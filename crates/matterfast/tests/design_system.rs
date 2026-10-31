//! The window is drawn with the design system alone (`gpui-adaptive`):
//! gpui-kit's components, its base and its assets are what the system is
//! made of, not what this application calls. `clippy.toml` names the
//! components one by one; this catches the rest, and a new one.

use std::path::Path;

const FORBIDDEN: [&str; 3] = ["gpui_kit::component", "gpui_kit::base", "gpui_kit::assets"];

fn sources(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("the sources can be listed") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

#[test]
fn nothing_but_the_design_system_draws_the_window() {
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    let offending: Vec<String> = files
        .iter()
        .flat_map(|path| {
            let text = std::fs::read_to_string(path).expect("a source can be read");
            text.lines()
                .enumerate()
                .filter(|(_, line)| FORBIDDEN.iter().any(|path| line.contains(path)))
                .map(|(number, line)| format!("{}:{}: {}", path.display(), number + 1, line.trim()))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(offending.is_empty(), "gpui-kit used directly:\n{}", offending.join("\n"));
}
