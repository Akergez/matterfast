/// What a form dialog looks like, apart from its fields.
pub struct FormSpec {
    pub title: String,
    pub note: String,
    pub ok: String,
    /// Derive the second text field from the first while it is untouched.
    pub follow: Option<fn(&str) -> String>,
}

impl FormSpec {
    pub fn new(title: &str, ok: &str) -> Self {
        FormSpec {
            title: title.to_string(),
            note: String::new(),
            ok: ok.to_string(),
            follow: None,
        }
    }
}
