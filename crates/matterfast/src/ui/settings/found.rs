use super::constants::SHOWN;
use crate::zed_extensions::Extension;

/// The extensions a search leaves, in the registry's order, and how many
/// there were before the list was cut to what is drawn.
pub(super) fn found<'a>(extensions: &'a [Extension], query: &str) -> (Vec<&'a Extension>, usize) {
    let needle = query.trim().to_lowercase();
    let matching: Vec<&Extension> = extensions
        .iter()
        .filter(|extension| extension.matches(&needle))
        .collect();
    let total = matching.len();
    (matching.into_iter().take(SHOWN).collect(), total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extension(id: &str, name: &str) -> Extension {
        Extension {
            id: id.into(),
            name: name.into(),
            description: None,
            authors: vec![],
            download_count: 0,
        }
    }

    #[test]
    fn a_search_keeps_the_registry_order_and_ignores_case_and_edges() {
        let extensions = [
            extension("catppuccin", "Catppuccin"),
            extension("tokyo-night", "Tokyo Night"),
            extension("catppuccin-blur", "Catppuccin Blur"),
        ];
        let (shown, total) = found(&extensions, "  CATP ");
        assert_eq!(total, 2);
        let ids: Vec<_> = shown.iter().map(|extension| extension.id.as_str()).collect();
        assert_eq!(ids, ["catppuccin", "catppuccin-blur"]);
        assert_eq!(found(&extensions, "").1, 3);
    }

    #[test]
    fn a_long_list_is_cut_and_says_how_long_it_was() {
        let extensions: Vec<Extension> = (0..SHOWN + 25)
            .map(|index| extension(&format!("theme-{index}"), "Theme"))
            .collect();
        let (shown, total) = found(&extensions, "theme");
        assert_eq!(shown.len(), SHOWN);
        assert_eq!(total, SHOWN + 25);
    }
}
