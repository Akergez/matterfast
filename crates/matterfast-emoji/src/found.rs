/// One emoji a search found.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    /// From the built-in table: its shortcode and its glyph.
    Unicode(&'static str, &'static str),
    /// One of the server's own, by name. Drawn as a picture.
    Custom(String),
}

impl Found {
    pub fn name(&self) -> &str {
        match self {
            Found::Unicode(name, _) => name,
            Found::Custom(name) => name,
        }
    }
}
