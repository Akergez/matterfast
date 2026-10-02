//! A scripted pair of hands, for looking at the interface without a human.
//!
//! `MATTERFAST_SCRIPT="wait:1500;click:120,300;key:ctrl-k;type:town;shot:switcher"`
//! plays those steps into the window once it opens. Nothing here is reachable
//! without the variable; it exists because a Wayland session gives a test no
//! other way to press a button and see what the screen looks like afterwards.
//!
//! Steps:
//! - `wait:<ms>`
//! - `click:<x>,<y>`, `rclick:<x>,<y>`, `move:<x>,<y>`
//! - `drag:<x>,<y>,<to-x>,<to-y>` — press, carry, let go
//! - `scroll:<x>,<y>,<dy>` — positive `dy` scrolls towards older content
//! - `key:<keystroke>` — `ctrl-k`, `escape`, `enter`
//! - `type:<text>`
//! - `shot:<name>` — runs `$MATTERFAST_SHOT_CMD <name>`
//! - `expect:<what>=<value>` — ends the process with status 1 unless it holds
//! - `quit`
//!
//! `expect` is what turns a script into a test (`tests/ui/` and
//! `build-aux/ui-tests.sh`): a click that lands beside its button changes
//! nothing, and without a check the script would still run to its `quit`. It
//! reads the session, not the picture, so it does not depend on fonts or on
//! how a compositor draws:
//! - `session=yes|no` — whether somebody is signed in
//! - `channel=<title>` — the open channel, as the sidebar names it
//! - `dialog=open|closed`
//! - `theme=light|dark`
//! - `theme-name=<name>` — the theme being worn, as the settings list it
//! - `search=<text>` — what is in the search box
//! - `search-hints=<a>|<b>|…` — the rows offered under it, empty when closed
//! - `search-hits=<n>` — how many messages the last search found
//! - `sidebar-width=<px>|auto`, `panel-width=<px>|auto` — what a side column
//!   was dragged to, or `auto` for one left at its share of the window
//!
//! Coordinates are divided by `MATTERFAST_SCRIPT_SCALE`, so they can be read
//! straight off a screenshot taken on a scaled display.

use std::collections::VecDeque;
use std::time::Duration;

use gpui_kit::component::{ActiveTheme as _, WindowExt as _};
use gpui_kit::{
    point, px, AnyWindowHandle, App, Keystroke, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, PlatformInput, Point, ScrollDelta, ScrollWheelEvent,
    TouchPhase, Window,
};

use crate::runtime;

#[derive(Debug, Clone, PartialEq)]
enum Step {
    Wait(u64),
    Click(f32, f32, bool),
    Move(f32, f32),
    Drag(f32, f32, f32, f32),
    Scroll(f32, f32, f32),
    Key(String),
    Type(String),
    Shot(String),
    Expect(String, String),
    Quit,
}

fn numbers(text: &str) -> Option<Vec<f32>> {
    text.split(',').map(|n| n.trim().parse().ok()).collect()
}

fn parse(script: &str) -> Vec<Step> {
    script
        .split(';')
        .filter_map(|step| {
            let step = step.trim();
            let (verb, rest) = step.split_once(':').unwrap_or((step, ""));
            Some(match verb {
                "wait" => Step::Wait(rest.parse().ok()?),
                "click" | "rclick" => match numbers(rest)?[..] {
                    [x, y] => Step::Click(x, y, verb == "rclick"),
                    _ => return None,
                },
                "move" => match numbers(rest)?[..] {
                    [x, y] => Step::Move(x, y),
                    _ => return None,
                },
                "drag" => match numbers(rest)?[..] {
                    [x, y, to_x, to_y] => Step::Drag(x, y, to_x, to_y),
                    _ => return None,
                },
                "scroll" => match numbers(rest)?[..] {
                    [x, y, dy] => Step::Scroll(x, y, dy),
                    _ => return None,
                },
                "key" => Step::Key(rest.to_string()),
                "type" => Step::Type(rest.to_string()),
                "shot" => Step::Shot(rest.to_string()),
                "expect" => {
                    let (what, value) = rest.split_once('=')?;
                    Step::Expect(what.trim().to_string(), value.trim().to_string())
                }
                "quit" => Step::Quit,
                _ => {
                    tracing::warn!(step, "unknown script step");
                    return None;
                }
            })
        })
        .collect()
}

/// Plays `MATTERFAST_SCRIPT` into `window`, if there is one to play.
pub fn play(window: AnyWindowHandle, cx: &mut App) {
    let Ok(script) = std::env::var("MATTERFAST_SCRIPT") else {
        return;
    };
    next(window, parse(&script).into(), cx);
}

/// Coordinates are read off screenshots, which are in device pixels;
/// `MATTERFAST_SCRIPT_SCALE` is the factor between those and the window's own.
fn at(x: f32, y: f32) -> Point<Pixels> {
    let scale = std::env::var("MATTERFAST_SCRIPT_SCALE")
        .ok()
        .and_then(|scale| scale.parse::<f32>().ok())
        .filter(|scale| *scale > 0.0)
        .unwrap_or(1.0);
    point(px(x / scale), px(y / scale))
}

fn pointer(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        PlatformInput::MouseMove(MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }),
        cx,
    );
}

/// What `expect:<what>` compares against, or `None` for a name it does not
/// know — which fails the script too, so a misspelt check cannot pass.
fn observe(what: &str, window: &mut Window, cx: &mut App) -> Option<String> {
    let ui = super::shell::current(cx);
    Some(match what {
        "session" => if ui.is_some() { "yes" } else { "no" }.to_string(),
        "channel" => ui
            .and_then(|ui| {
                let state = ui.state.borrow();
                let channel = state.channel(state.current_channel.as_deref()?)?;
                Some(state.channel_title(channel))
            })
            .unwrap_or_default(),
        "dialog" => if window.has_active_dialog(cx) { "open" } else { "closed" }.to_string(),
        "theme" => if cx.theme().mode.is_dark() { "dark" } else { "light" }.to_string(),
        "theme-name" => cx.theme().theme_name().to_string(),
        "search" => ui.map(|ui| ui.search_box.text(cx)).unwrap_or_default(),
        "search-hints" => ui.map(|ui| ui.search_box.offered().join("|")).unwrap_or_default(),
        "search-hits" => ui
            .map(|ui| ui.state.borrow().search_results.len())
            .unwrap_or(0)
            .to_string(),
        "sidebar-width" | "panel-width" => {
            let divider = if what == "sidebar-width" {
                super::Divider::Sidebar
            } else {
                super::Divider::Panel
            };
            ui.and_then(|ui| ui.widths.get(divider))
                .map_or("auto".to_string(), |width| width.round().to_string())
        }
        _ => return None,
    })
}

fn next(handle: AnyWindowHandle, mut steps: VecDeque<Step>, cx: &mut App) {
    let Some(step) = steps.pop_front() else {
        return;
    };
    // Every step is followed by a short pause: what a step does is usually
    // queued rather than done, and the next one should see its result.
    let mut pause = 120;
    tracing::info!(?step, "script");
    let _ = handle.update(cx, |_, window, cx| match &step {
        Step::Wait(ms) => pause = *ms,
        Step::Move(x, y) => pointer(window, at(*x, *y), cx),
        Step::Click(x, y, right) => {
            let position = at(*x, *y);
            let button = if *right {
                MouseButton::Right
            } else {
                MouseButton::Left
            };
            pointer(window, position, cx);
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }),
                cx,
            );
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                }),
                cx,
            );
        }
        Step::Drag(x, y, to_x, to_y) => {
            let (from, to) = (at(*x, *y), at(*to_x, *to_y));
            pointer(window, from, cx);
            window.dispatch_event(
                PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }),
                cx,
            );
            // Halfway first: a drag begins on the move that leaves the place
            // it was pressed, and only the moves after that one carry it.
            let halfway = point((from.x + to.x) / 2., (from.y + to.y) / 2.);
            for position in [halfway, to, to] {
                window.dispatch_event(
                    PlatformInput::MouseMove(MouseMoveEvent {
                        position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Modifiers::default(),
                    }),
                    cx,
                );
            }
            window.dispatch_event(
                PlatformInput::MouseUp(MouseUpEvent {
                    button: MouseButton::Left,
                    position: to,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                }),
                cx,
            );
        }
        Step::Scroll(x, y, dy) => {
            pointer(window, at(*x, *y), cx);
            window.dispatch_event(
                PlatformInput::ScrollWheel(ScrollWheelEvent {
                    position: at(*x, *y),
                    delta: ScrollDelta::Pixels(point(px(0.), px(*dy))),
                    modifiers: Modifiers::default(),
                    touch_phase: TouchPhase::Moved,
                }),
                cx,
            );
        }
        Step::Key(key) => match Keystroke::parse(key) {
            Ok(keystroke) => {
                window.dispatch_keystroke(keystroke, cx);
            }
            Err(error) => tracing::warn!(%error, key, "not a keystroke"),
        },
        Step::Type(text) => {
            for letter in text.chars() {
                let keystroke = Keystroke {
                    modifiers: Modifiers::default(),
                    key: letter.to_string(),
                    key_char: Some(letter.to_string()),
                };
                window.dispatch_keystroke(keystroke, cx);
            }
        }
        Step::Shot(name) => {
            if let Ok(command) = std::env::var("MATTERFAST_SHOT_CMD") {
                let _ = std::process::Command::new(command).arg(name).status();
            }
        }
        Step::Expect(what, value) => {
            let found = observe(what, window, cx);
            if found.as_deref() != Some(value) {
                // Straight out, with a status the runner can see: the steps
                // after a failed check would only be acting on the wrong screen.
                eprintln!("script: expected {what}={value}, found {found:?}");
                std::process::exit(1);
            }
        }
        Step::Quit => cx.quit(),
    });
    runtime::after(Duration::from_millis(pause), move |cx| {
        next(handle, steps, cx)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_is_steps_between_semicolons() {
        assert_eq!(
            parse("wait:200; click:10,20 ;rclick:1,2;key:ctrl-k;type:a:b;shot:x;expect:channel=Town Square;quit"),
            [
                Step::Wait(200),
                Step::Click(10.0, 20.0, false),
                Step::Click(1.0, 2.0, true),
                Step::Key("ctrl-k".into()),
                Step::Type("a:b".into()),
                Step::Shot("x".into()),
                Step::Expect("channel".into(), "Town Square".into()),
                Step::Quit,
            ]
        );
    }

    #[test]
    fn a_step_that_does_not_parse_is_skipped() {
        assert_eq!(parse("click:1;bogus;wait:x;scroll:1,2,3"), [Step::Scroll(1.0, 2.0, 3.0)]);
    }
}
