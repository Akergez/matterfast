/// The one extension the fake registry has. Its theme is in Zed's format,
/// trailing comma and comment included, because published themes have them.
pub(super) const ZED_THEME: &str = r##"{
  // A theme nobody else has, so a test cannot pass by finding a built-in one.
  "name": "Testserver",
  "author": "matterfast-testserver",
  "themes": [
    {
      "name": "Testserver Dusk",
      "appearance": "dark",
      "style": {
        "background": "#1b1d2aff",
        "editor.background": "#14151fff",
        "panel.background": "#1b1d2aff",
        "elevated_surface.background": "#22243355",
        "element.background": "#262a3cff",
        "element.hover": "#30354bff",
        "ghost_element.hover": "#30354b80",
        "text": "#d6d9eaff",
        "text.muted": "#8f94b3ff",
        "text.accent": "#f2a65aff",
        "border": "#30354bff",
        "syntax": { "comment": { "color": "#8f94b3ff", "font_style": "italic" } },
      }
    }
  ]
}"##;
