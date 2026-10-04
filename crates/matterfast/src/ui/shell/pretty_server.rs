/// A server URL with the scheme stripped, which is how people say it.
pub(super) fn pretty_server(url: &str) -> String {
    url.trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_is_named_the_way_people_say_it() {
        assert_eq!(pretty_server("https://mm.example.com/"), "mm.example.com");
        assert_eq!(pretty_server("http://localhost:8065"), "localhost:8065");
    }
}
