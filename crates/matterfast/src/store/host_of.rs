use super::error::Error;

/// One database per server, named after its host.
///
/// The host reaches us from a text entry the user typed into, so it is treated
/// as hostile: anything that is not a letter, digit, dot or dash becomes `_`,
/// which leaves no separator, no `..` and no absolute path that could put the
/// file somewhere other than `dir`.
pub(super) fn host_of(server: &str) -> Result<String, Error> {
    let rest = server.split_once("://").map_or(server, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    // `[::1]:8065` keeps its brackets around the address; everything else stops
    // at the port separator.
    let host = match authority.strip_prefix('[').and_then(|h| h.split_once(']')) {
        Some((inside, _)) => inside,
        None => authority.split(':').next().unwrap_or(""),
    };
    let clean: String = host
        .chars()
        .take(100)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    // Rejects "", "." and ".." — the three names that would not be a new file.
    if clean.trim_matches('.').is_empty() {
        return Err(Error::BadServer(server.to_string()));
    }
    Ok(clean)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_cannot_name_a_file_outside_the_directory() {
        assert_eq!(
            host_of("https://mm.example.com/").unwrap(),
            "mm.example.com"
        );
        assert_eq!(
            host_of("https://MM.Example.com:8065").unwrap(),
            "mm.example.com"
        );
        assert_eq!(host_of("mm.example.com").unwrap(), "mm.example.com");
        assert_eq!(
            host_of("https://user:pw@mm.example.com").unwrap(),
            "mm.example.com"
        );
        assert_eq!(host_of("http://[::1]:8065").unwrap(), "__1");
        // The interesting ones: nothing here escapes `dir`.
        assert_eq!(host_of("https://a/../../etc/passwd").unwrap(), "a");
        assert!(!host_of("https://..%2f..%2fetc").unwrap().contains('/'));
        assert!(host_of("https://").is_err());
        assert!(host_of("https://../").is_err());
    }
}
