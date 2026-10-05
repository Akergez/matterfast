//! Where a frame's time goes, for a build that feels slow on somebody's
//! machine and not on ours.
//!
//! `MATTERFAST_FRAME_LOG=1` prints a line to stderr for every frame drawn:
//!
//! ```text
//! frame 14.2ms (+16.7) | sidebar kept | chat build 0.9 layout 3.1 prepaint 7.4 paint 1.2 rows 23/5.0 | right kept
//! ```
//!
//! `frame` is from the window starting to build its elements to the last of
//! them being painted; the figure in brackets is how long ago the previous
//! frame started. A column is `kept` when it was not drawn again. Otherwise:
//! `build` is making its elements, `layout` is measuring them, `prepaint` is
//! placing them — which is where a list builds and measures its rows — and
//! `paint` is drawing. `rows` is how many list rows were built in the frame
//! and what building them took, which is part of `prepaint`.
//!
//! What the toolkit does after the last element is painted — handing the
//! scene to the GPU and waiting for the display — is not in here.
//! `ZED_MEASUREMENTS=1` prints a `frame duration` that does include it, so the
//! two together say which side of that line the time is on.

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{canvas, AnyElement};

use super::Part;

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("MATTERFAST_FRAME_LOG").is_some())
}

#[derive(Default, Clone, Copy)]
struct Lap {
    build: Option<(Instant, Instant)>,
    laid_out: Option<Instant>,
    prepainted: Option<Instant>,
    paint_began: Option<Instant>,
    painted: Option<Instant>,
    rows: usize,
    rows_took: Duration,
}

#[derive(Default)]
struct Log {
    began: Option<Instant>,
    previous: Option<Instant>,
    laps: [Lap; 3],
}

thread_local! {
    static LOG: RefCell<Log> = RefCell::default();
}

fn column(part: Part) -> Option<usize> {
    match part {
        Part::Sidebar => Some(0),
        Part::Chat => Some(1),
        Part::Right => Some(2),
        Part::Frame => None,
    }
}

fn lap(part: Part, note: impl FnOnce(&mut Lap)) {
    let Some(index) = column(part) else { return };
    LOG.with(|log| note(&mut log.borrow_mut().laps[index]));
}

fn ms(from: Instant, to: Instant) -> f64 {
    to.saturating_duration_since(from).as_secs_f64() * 1000.
}

/// The window has started on a frame.
pub fn begin() {
    if !enabled() {
        return;
    }
    LOG.with(|log| {
        let mut log = log.borrow_mut();
        log.previous = log.began.replace(Instant::now());
        log.laps = Default::default();
    });
}

/// A column's elements were made between these two moments.
pub fn built(part: Part, from: Instant) {
    lap(part, |lap| lap.build = Some((from, Instant::now())));
}

/// A row of a list in this column was made, in this long.
pub fn row(part: Part, from: Instant) {
    lap(part, |lap| {
        lap.rows += 1;
        lap.rows_took += from.elapsed();
    });
}

/// Something that takes no room and notes when the toolkit got to it. One
/// goes before a column's content and one after, and the gaps are the phases.
pub fn mark(part: Part, last: bool) -> AnyElement {
    canvas(
        move |_, _, _| {
            let now = Some(Instant::now());
            lap(part, |lap| match last {
                false => lap.laid_out = now,
                true => lap.prepainted = now,
            });
        },
        move |_, _, _, _| {
            let now = Some(Instant::now());
            lap(part, |lap| match last {
                false => lap.paint_began = now,
                true => lap.painted = now,
            });
        },
    )
    .absolute()
    .size_0()
    .into_any_element()
}

/// The last thing in the window: painting it is the end of the frame.
pub fn end() -> AnyElement {
    canvas(|_, _, _| (), |_, _, _, _| print())
        .absolute()
        .size_0()
        .into_any_element()
}

fn print() {
    LOG.with(|log| {
        let log = log.borrow();
        let Some(began) = log.began else { return };
        let mut line = format!("frame {:.1}ms", ms(began, Instant::now()));
        if let Some(previous) = log.previous {
            line += &format!(" (+{:.1})", ms(previous, began));
        }
        for (name, lap) in ["sidebar", "chat", "right"].iter().zip(&log.laps) {
            let Some((from, to)) = lap.build else {
                line += &format!(" | {name} kept");
                continue;
            };
            line += &format!(" | {name} build {:.1}", ms(from, to));
            if let Some(laid_out) = lap.laid_out {
                line += &format!(" layout {:.1}", ms(to, laid_out));
                if let Some(prepainted) = lap.prepainted {
                    line += &format!(" prepaint {:.1}", ms(laid_out, prepainted));
                }
            }
            if let (Some(from), Some(to)) = (lap.paint_began, lap.painted) {
                line += &format!(" paint {:.1}", ms(from, to));
            }
            if lap.rows > 0 {
                line += &format!(
                    " rows {}/{:.1}",
                    lap.rows,
                    lap.rows_took.as_secs_f64() * 1000.
                );
            }
        }
        eprintln!("{line}");
    });
}
