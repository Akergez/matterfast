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
//!
//! After the brackets, `wheel 3/21.4` is three turns of the wheel answered by
//! this frame, the first of which had waited 21.4 ms when it began.
//!
//! A column says that a list is slow and not what in its rows is, so the
//! pieces of a message row are counted as well, after the columns:
//!
//! ```text
//! || row 10: layout 3.1 prepaint 9.0 paint 1.2 | body 10: layout 2.4 prepaint 7.7 paint 0.9
//! ```
//!
//! The number after a name is how many of them the frame drew, and the times
//! are for all of them together. `row` is a whole row of the conversation and
//! holds the pieces named after it, wherever a message is drawn. `layout`
//! here is asking for the layout only: the toolkit measures text when it
//! solves the layout of a row as a whole, which belongs to no piece — it is
//! what is left of the column's `prepaint` once the rows are taken out of it.
//!
//! And between the end of one frame and the start of the next, which is
//! where a frame that was quick to draw can still be late:
//!
//! ```text
//! frame 14.2ms (+50.1, gap 35.9 busy 21.4) | …
//! ```
//!
//! `gap` is that stretch on the clock and `busy` is how much of it this
//! thread spent running, as the kernel counts it — to within a scheduler
//! tick, so a few milliseconds either way. A gap that is nearly all `busy` is
//! work done on this thread between frames, of which handing the scene over
//! is a few milliseconds; one that is not is the thread waiting to be asked
//! for a frame.

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui_kit::prelude::*;
use gpui_kit::{
    canvas, AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    LayoutId, Pixels, Window,
};

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

/// What every piece of one name took in a frame.
#[derive(Default, Clone, Copy)]
struct Tally {
    count: usize,
    layout: Duration,
    prepaint: Duration,
    paint: Duration,
}

#[derive(Default)]
struct Log {
    began: Option<Instant>,
    previous: Option<Instant>,
    /// When the frame before this one was painted, and what the thread had
    /// run for by then.
    ended: Option<(Instant, Option<Duration>)>,
    /// The gap since then: on the clock, and running.
    gap: Option<(Duration, Option<Duration>)>,
    /// The turns of the wheel that no frame has shown yet: when the first of
    /// them came, and how many there have been.
    wheel: Option<(Instant, usize)>,
    /// The same for the frame being drawn: how many it answers, and how long
    /// the first had waited when the frame began.
    answered: Option<(usize, Duration)>,
    laps: [Lap; 3],
    /// In the order the pieces were first met, which is the order they are
    /// drawn in.
    pieces: Vec<(&'static str, Tally)>,
    /// Where the whole window was asked to be drawn again from, since the
    /// last frame.
    asked: Vec<&'static std::panic::Location<'static>>,
}

/// Every column is about to be drawn again, and this is where that was asked
/// for. The next frame's line ends with it, after a `!`: a column that was
/// built in a frame nobody touched it in was built for one of these.
pub fn asked(from: &'static std::panic::Location<'static>) {
    if enabled() {
        LOG.with(|log| log.borrow_mut().asked.push(from));
    }
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
        let now = Instant::now();
        log.previous = log.began.replace(now);
        log.gap = log.ended.take().map(|(ended, ran)| {
            let busy = ran.zip(thread_ran()).map(|(then, now)| now.saturating_sub(then));
            (now.saturating_duration_since(ended), busy)
        });
        log.answered = log
            .wheel
            .take()
            .map(|(first, turns)| (turns, now.saturating_duration_since(first)));
        log.laps = Default::default();
        log.pieces.clear();
    });
}

/// The conversation was scrolled. A list only draws when it is moved, so a
/// frame that comes 33 ms after the last may be late or may simply be the
/// next thing that happened; how long the first turn of the wheel waited for
/// its frame tells the two apart.
pub fn wheel() {
    if !enabled() {
        return;
    }
    LOG.with(|log| {
        let wheel = &mut log.borrow_mut().wheel;
        match wheel {
            Some((_, turns)) => *turns += 1,
            None => *wheel = Some((Instant::now(), 1)),
        }
    });
}

/// How long this thread has been running, in all. The kernel keeps it in
/// nanoseconds, first on the line; elsewhere there is nothing as cheap to ask.
fn thread_ran() -> Option<Duration> {
    let stat = std::fs::read_to_string("/proc/thread-self/schedstat").ok()?;
    let ran = stat.split_whitespace().next()?.parse().ok()?;
    Some(Duration::from_nanos(ran))
}

fn tally(what: &'static str, note: impl FnOnce(&mut Tally)) {
    LOG.with(|log| {
        let pieces = &mut log.borrow_mut().pieces;
        let index = match pieces.iter().position(|(name, _)| *name == what) {
            Some(index) => index,
            None => {
                pieces.push((what, Tally::default()));
                pieces.len() - 1
            }
        };
        note(&mut pieces[index].1);
    });
}

/// A piece of a row, counted under this name. It is the element it was given
/// when nothing is being logged.
pub fn timed(what: &'static str, element: impl IntoElement) -> AnyElement {
    let element = element.into_any_element();
    if !enabled() {
        return element;
    }
    Timed { what, element }.into_any_element()
}

/// An element that is its child in every way, and notes what the child took.
/// It asks for no layout of its own, so the child sits where it would have.
struct Timed {
    what: &'static str,
    element: AnyElement,
}

impl IntoElement for Timed {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Timed {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let began = Instant::now();
        let layout = self.element.request_layout(window, cx);
        tally(self.what, |tally| {
            tally.count += 1;
            tally.layout += began.elapsed();
        });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let began = Instant::now();
        self.element.prepaint(window, cx);
        tally(self.what, |tally| tally.prepaint += began.elapsed());
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let began = Instant::now();
        self.element.paint(window, cx);
        tally(self.what, |tally| tally.paint += began.elapsed());
    }
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
        let mut log = log.borrow_mut();
        let Some(began) = log.began else { return };
        let now = Instant::now();
        log.ended = Some((now, thread_ran()));
        let mut line = format!("frame {:.1}ms", ms(began, now));
        if let Some(previous) = log.previous {
            line += &format!(" (+{:.1}", ms(previous, began));
            if let Some((gap, busy)) = log.gap {
                line += &format!(", gap {:.1}", gap.as_secs_f64() * 1000.);
                if let Some(busy) = busy {
                    line += &format!(" busy {:.1}", busy.as_secs_f64() * 1000.);
                }
            }
            line += ")";
        }
        if let Some((turns, waited)) = log.answered {
            line += &format!(" wheel {turns}/{:.1}", waited.as_secs_f64() * 1000.);
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
        let took = |took: Duration| took.as_secs_f64() * 1000.;
        for (index, (name, tally)) in log.pieces.iter().enumerate() {
            line += if index == 0 { " || " } else { " | " };
            line += &format!(
                "{name} {}: layout {:.1} prepaint {:.1} paint {:.1}",
                tally.count,
                took(tally.layout),
                took(tally.prepaint),
                took(tally.paint)
            );
        }
        if !log.asked.is_empty() {
            // The file's name is enough to find it by; the path to it is
            // most of a line.
            let asked: Vec<String> = log
                .asked
                .drain(..)
                .map(|from| {
                    let file = from.file().rsplit(['/', '\\']).next().unwrap_or_default();
                    format!("{file}:{}", from.line())
                })
                .collect();
            line += &format!(" ! {}", asked.join(", "));
        }
        eprintln!("{line}");
    });
}
