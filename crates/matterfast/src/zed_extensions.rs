//! Zed's extension registry, as a place to get themes from.
//!
//! There is no repository of Zed themes to clone: the registry is a list of
//! several hundred other people's repositories, and what the editor itself
//! talks to is an API in front of them — one request for the list, one for a
//! packaged extension. This module makes those two requests and nothing else.
//! It hands back the theme files as text, without their names; what they mean is
//! [`crate::zed_theme`]'s business and where they are kept is
//! [`crate::themes`]'s.
//!
//! The API is the editor's own and promises nothing to anybody else, so every
//! failure here is an ordinary one to be shown and survived: the themes that
//! ship with the application and the ones already installed do not depend on
//! it.
//!
//! What comes back was written by strangers. The archive is unpacked in
//! memory and only the text of `themes/*.json` is taken out of it: no path in
//! it is ever used as a path here. Both the download and each file have a
//! size they may not exceed — a theme is a few dozen kilobytes.

use std::io::Read;

use serde::Deserialize;

/// Where the registry is, unless `MATTERFAST_THEMES_API` says otherwise.
const API: &str = "https://api.zed.dev";
const MAX_ARCHIVE: usize = 8 * 1024 * 1024;
const MAX_THEME: u64 = 2 * 1024 * 1024;
const MAX_THEMES: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Http(#[from] reqwest::Error),
    #[error("the extension is larger than a theme has any reason to be")]
    TooLarge,
    #[error("the extension could not be unpacked: {0}")]
    Archive(#[from] std::io::Error),
    #[error("there are no themes in this extension")]
    NoThemes,
    #[error("\"{0}\" is not an extension name")]
    BadId(String),
}

/// One entry of the registry, as much of it as a list row shows.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Extension {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub download_count: u64,
}

impl Extension {
    /// Whether `needle`, already lowercased, is somewhere a person would
    /// look for it.
    pub fn matches(&self, needle: &str) -> bool {
        needle.is_empty()
            || self.name.to_lowercase().contains(needle)
            || self.id.contains(needle)
            || self
                .description
                .as_deref()
                .is_some_and(|text| text.to_lowercase().contains(needle))
    }

    /// The first author without the address: registry entries are written
    /// as `Name <mail>`.
    pub fn author(&self) -> Option<&str> {
        let author = self.authors.first()?;
        let name = author.split('<').next().unwrap_or(author).trim();
        (!name.is_empty()).then_some(name)
    }
}

/// An extension name is also a directory name here, so it may only be made
/// of what the registry itself allows in one.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn api() -> String {
    std::env::var("MATTERFAST_THEMES_API")
        .ok()
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| API.to_string())
}

fn http() -> Result<reqwest::Client, Error> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("matterfast/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

/// Every extension that provides themes, most downloaded first — the order
/// the registry answers in, and the only measure of quality there is.
pub async fn list() -> Result<Vec<Extension>, Error> {
    #[derive(Deserialize)]
    struct Page {
        data: Vec<Extension>,
    }
    let page: Page = http()?
        .get(format!("{}/extensions", api()))
        // What the editor sends: without a schema version the registry
        // answers with extensions in a packaging this cannot read.
        .query(&[("max_schema_version", "1"), ("provides", "themes")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(usable(page.data))
}

/// What of the registry's answer can be installed: an entry whose name could
/// not be a directory is left out rather than offered and refused.
fn usable(extensions: Vec<Extension>) -> Vec<Extension> {
    extensions
        .into_iter()
        .filter(|extension| valid_id(&extension.id))
        .collect()
}

/// The theme files of one extension, latest version.
pub async fn download(id: &str) -> Result<Vec<String>, Error> {
    if !valid_id(id) {
        return Err(Error::BadId(id.to_string()));
    }
    let mut response = http()?
        .get(format!("{}/extensions/{id}/download", api()))
        .send()
        .await?
        .error_for_status()?;
    // The length a server claims is not the length it sends, so the body is
    // counted as it arrives.
    let mut archive = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if archive.len() + chunk.len() > MAX_ARCHIVE {
            return Err(Error::TooLarge);
        }
        archive.extend_from_slice(&chunk);
    }
    unpack(&archive)
}

/// The `themes/*.json` of a gzipped tarball.
fn unpack(archive: &[u8]) -> Result<Vec<String>, Error> {
    let mut files = Vec::new();
    let mut tarball = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    for entry in tarball.entries()? {
        let entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.into_owned();
        let in_themes = path
            .parent()
            .and_then(|parent| parent.file_name())
            .is_some_and(|directory| directory == "themes");
        let is_json = path.extension().is_some_and(|extension| extension == "json");
        if !in_themes || !is_json {
            continue;
        }
        if entry.size() > MAX_THEME || files.len() == MAX_THEMES {
            return Err(Error::TooLarge);
        }
        let mut text = String::new();
        // `take`, because the size in a header is a claim like any other.
        entry.take(MAX_THEME).read_to_string(&mut text)?;
        files.push(text);
    }
    if files.is_empty() {
        return Err(Error::NoThemes);
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tarball(entries: &[(&str, &str)]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (path, text) in entries {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, text.as_bytes()).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap()
    }

    #[test]
    fn only_the_theme_files_come_out_of_an_extension() {
        let archive = tarball(&[
            ("./extension.toml", "id = \"x\""),
            ("./themes/night.json", "{\"a\": 1}"),
            ("./themes/README.md", "hello"),
            ("./src/other.json", "{}"),
            ("./themes/day.json", "{\"b\": 2}"),
        ]);
        let files = unpack(&archive).unwrap();
        assert_eq!(files, ["{\"a\": 1}", "{\"b\": 2}"]);
    }

    #[test]
    fn an_extension_without_themes_is_an_error() {
        let archive = tarball(&[("./extension.toml", "id = \"x\"")]);
        assert!(matches!(unpack(&archive), Err(Error::NoThemes)));
    }

    #[test]
    fn what_is_not_an_archive_is_an_error_and_not_a_panic() {
        assert!(matches!(unpack(b"<html>not found</html>"), Err(Error::Archive(_))));
    }

    #[test]
    fn an_extension_name_cannot_leave_its_directory() {
        assert!(valid_id("catppuccin"));
        assert!(valid_id("tokyo-night_2"));
        for id in ["", "..", "a/b", "../x", "a.b", "Caps", "a b", "/etc"] {
            assert!(!valid_id(id), "{id:?}");
        }
    }

    #[test]
    fn the_registry_answer_is_read_and_unusable_names_are_dropped() {
        #[derive(Deserialize)]
        struct Page {
            data: Vec<Extension>,
        }
        let page: Page = serde_json::from_str(
            r#"{"data": [
                {"id": "catppuccin", "name": "Catppuccin", "version": "0.2.27",
                 "description": "Soothing pastel theme", "authors": ["Catppuccin <r@c.com>"],
                 "provides": ["themes"], "download_count": 1163124},
                {"id": "../evil", "name": "Evil"},
                {"id": "bare"}
            ]}"#,
        )
        .unwrap();
        let extensions = usable(page.data);
        assert_eq!(extensions.len(), 2);
        assert_eq!(extensions[0].author(), Some("Catppuccin"));
        assert_eq!(extensions[0].download_count, 1163124);
        assert_eq!(extensions[1].author(), None);
    }

    #[test]
    fn a_search_looks_at_the_name_the_id_and_the_description() {
        let extension = Extension {
            id: "tokyo-night".into(),
            name: "Tokyo Night Themes".into(),
            description: Some("A clean, dark theme".into()),
            authors: vec![],
            download_count: 0,
        };
        assert!(extension.matches(""));
        assert!(extension.matches("tokyo night"));
        assert!(extension.matches("tokyo-night"));
        assert!(extension.matches("clean"));
        assert!(!extension.matches("gruvbox"));
    }
}
