use crate::ui::dialogs::levels::level_index;

/// One field of a form.
pub enum Field {
    Text {
        label: String,
        value: String,
        placeholder: String,
    },
    Switch {
        label: String,
        subtitle: String,
        on: bool,
    },
    /// One of a fixed set, as (id, label). The value is the id.
    Choice {
        label: String,
        options: Vec<(String, String)>,
        selected: usize,
    },
}

impl Field {
    pub fn text(label: &str, value: &str) -> Field {
        Field::Text {
            label: label.to_string(),
            value: value.to_string(),
            placeholder: String::new(),
        }
    }

    pub fn switch(label: &str, subtitle: &str, on: bool) -> Field {
        Field::Switch {
            label: label.to_string(),
            subtitle: subtitle.to_string(),
            on,
        }
    }

    pub fn choice(label: &str, options: &[(&str, &str)], current: &str) -> Field {
        Field::Choice {
            label: label.to_string(),
            options: options
                .iter()
                .map(|(id, label)| (id.to_string(), label.to_string()))
                .collect(),
            selected: level_index(options, current),
        }
    }
}
