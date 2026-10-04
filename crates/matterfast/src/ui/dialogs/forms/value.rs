/// What a form answered, field by field, in the order the fields were given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    Bool(bool),
    Choice(String),
}

impl Value {
    pub fn text(&self) -> String {
        match self {
            Value::Text(text) | Value::Choice(text) => text.clone(),
            Value::Bool(on) => on.to_string(),
        }
    }

    pub fn bool(&self) -> bool {
        matches!(self, Value::Bool(true))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_form_value_reads_as_what_it_holds() {
        assert_eq!(Value::Text("a".into()).text(), "a");
        assert_eq!(Value::Choice("all".into()).text(), "all");
        assert!(Value::Bool(true).bool());
        assert!(!Value::Text("true".into()).bool());
    }
}
