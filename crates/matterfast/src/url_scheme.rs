//! Telling Windows that `mattermost-dev://` links belong to this program.
//!
//! A single sign-on ends with the browser opening such a link
//! ([`crate::ui::sso`]). On Linux the desktop file says who handles it; on
//! Windows that is a few values under `HKEY_CURRENT_USER\Software\Classes`,
//! and with no installer to write them, the application writes them itself
//! every time it starts. That needs no administrator, and it keeps the entry
//! pointing at the executable wherever the zip was unpacked or moved to —
//! an entry written once goes stale the first time the folder is renamed.
//!
//! The link then starts this program a second time, with the link as its
//! argument, and that copy hands it to the running one ([`crate::ipc`]).

/// The scheme the server redirects a desktop sign-in to. See `sso.rs` for why
/// it is not plain `mattermost`.
#[cfg(windows)]
const SCHEME: &str = "mattermost-dev";

/// The command Windows runs for a link: this executable, the link as its one
/// argument. Both quoted — a path has spaces more often than not, and an
/// unquoted `%1` lets a link with a space in it become several arguments.
fn command(executable: &str) -> String {
    format!("\"{executable}\" \"%1\"")
}

/// Makes this executable the handler of the scheme for the person signed in.
/// A failure is logged and survived: everything but single sign-on works
/// without it.
#[cfg(windows)]
pub fn register() {
    if let Err(error) = register_as(SCHEME) {
        tracing::warn!(%error, "could not register the sign-in link scheme");
    }
}

#[cfg(windows)]
fn register_as(scheme: &str) -> Result<(), Box<dyn std::error::Error>> {
    use windows_registry::CURRENT_USER;

    let executable = std::env::current_exe()?;
    let executable = executable.to_str().ok_or("the executable's path is not text")?;
    let class = CURRENT_USER.create(format!(r"Software\Classes\{scheme}"))?;
    class.set_string("", "URL:Matterfast sign-in")?;
    // The presence of this value, not what is in it, is what makes the class
    // a URL scheme.
    class.set_string("URL Protocol", "")?;
    class
        .create(r"shell\open\command")?
        .set_string("", command(executable))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_quotes_the_path_and_the_link() {
        assert_eq!(
            command(r"C:\Program Files\Matterfast\matterfast.exe"),
            r#""C:\Program Files\Matterfast\matterfast.exe" "%1""#,
        );
    }

    /// Under a scheme of its own, so that running the tests on a machine
    /// where the application is used does not hand its links to a test.
    #[cfg(windows)]
    #[test]
    fn registering_writes_what_windows_reads_to_open_a_link() {
        use windows_registry::CURRENT_USER;

        let scheme = format!("matterfast-test-{}", std::process::id());
        register_as(&scheme).unwrap();

        let class = CURRENT_USER.open(format!(r"Software\Classes\{scheme}")).unwrap();
        assert_eq!(class.get_string("URL Protocol").unwrap(), "");
        let executable = std::env::current_exe().unwrap();
        assert_eq!(
            class.open(r"shell\open\command").unwrap().get_string("").unwrap(),
            command(executable.to_str().unwrap()),
        );

        CURRENT_USER.remove_tree(format!(r"Software\Classes\{scheme}")).unwrap();
    }
}
