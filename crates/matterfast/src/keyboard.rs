//! Telling a phone's keyboard what the box it is typing into is for.
//!
//! The platform layer asks Android for the same plain keyboard whatever has
//! the focus, so a message got no capital at the start of a sentence and no
//! corrections, and an address or a password got a keyboard for prose. Only
//! the application knows which box is which, and this is where it says so:
//! `GpuiInputActivity.gpuiKeyboardHint` keeps the last thing said and shapes
//! the keyboard by it.
//!
//! A box says what it is for when it gets the focus and takes it back when
//! it loses it. The two do not come in a fixed order when the focus goes from
//! one box to another, so who spoke last is remembered: a box that lost the
//! focus only takes back what is still its own.
//!
//! Off Android there is no keyboard to tell and all of this does nothing.

use std::cell::Cell;

use gpui_kit::component::input::InputEvent;
use gpui_kit::EntityId;

/// What a box is for. The numbers are the ones `gpuiKeyboardHint` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// Nothing in particular: the plain keyboard, as before.
    Plain = 0,
    /// A message: sentences, capitals, corrections.
    Message = 1,
    /// The address of a server.
    Address = 2,
    /// The name or the e-mail address somebody signs in with.
    Login = 3,
    Password = 4,
}

thread_local! {
    /// The box that last said what it is for.
    static SPOKE: Cell<Option<EntityId>> = const { Cell::new(None) };
}

/// What the keyboard should be told after `event` in the box `id`, if
/// anything, given who spoke last. Kept apart from the telling so that it
/// can be tested where there is no keyboard.
fn told(
    spoke: Option<EntityId>,
    id: EntityId,
    event: &InputEvent,
    purpose: Purpose,
) -> Option<(Option<EntityId>, Purpose)> {
    match event {
        InputEvent::Focus => Some((Some(id), purpose)),
        // Another box has spoken since: the keyboard is already its.
        InputEvent::Blur if spoke == Some(id) => Some((None, Purpose::Plain)),
        _ => None,
    }
}

/// Follows a box's focus: call it with every event of a box that is for
/// something.
pub fn follow(id: EntityId, event: &InputEvent, purpose: Purpose) {
    let Some((spoke, purpose)) = told(SPOKE.get(), id, event, purpose) else {
        return;
    };
    SPOKE.set(spoke);
    hint(purpose);
}

#[cfg(not(target_os = "android"))]
fn hint(_: Purpose) {}

#[cfg(target_os = "android")]
fn hint(purpose: Purpose) {
    use gpui_mobile::android::jni as bridge;
    use jni::objects::JValue;

    let told = bridge::with_env(|env| {
        let activity = bridge::activity(env)?;
        env.call_method(
            &activity,
            jni::jni_str!("gpuiKeyboardHint"),
            jni::jni_sig!("(I)V"),
            &[JValue::Int(purpose as i32)],
        )
        .map_err(|e| {
            env.exception_clear();
            e.to_string()
        })?;
        Ok(())
    });
    if let Err(error) = told {
        tracing::warn!(%error, ?purpose, "could not tell the keyboard what a box is for");
    }
}

#[cfg(test)]
mod tests {
    use gpui_kit::component::input::InputEvent;
    use gpui_kit::EntityId;

    use super::{told, Purpose};

    fn id(n: u64) -> EntityId {
        EntityId::from(n)
    }

    #[test]
    fn a_box_that_gets_the_focus_says_what_it_is_for() {
        assert_eq!(
            told(None, id(1), &InputEvent::Focus, Purpose::Message),
            Some((Some(id(1)), Purpose::Message))
        );
    }

    #[test]
    fn a_box_that_loses_the_focus_takes_it_back() {
        assert_eq!(
            told(Some(id(1)), id(1), &InputEvent::Blur, Purpose::Message),
            Some((None, Purpose::Plain))
        );
    }

    #[test]
    fn losing_the_focus_to_a_box_that_already_spoke_changes_nothing() {
        // The thread's box got the focus first, and then the conversation's
        // heard that it had lost it.
        assert_eq!(
            told(Some(id(2)), id(1), &InputEvent::Blur, Purpose::Message),
            None
        );
    }

    #[test]
    fn typing_says_nothing() {
        assert_eq!(
            told(Some(id(1)), id(1), &InputEvent::Change, Purpose::Message),
            None
        );
    }
}
