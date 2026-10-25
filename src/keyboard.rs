//! The on-screen keyboard: what a box is for, raising it on a tap, and how
//! much of the window it covers.
//!
//! gpui-mobile asks Android for the same plain keyboard whatever has the
//! focus, so a message gets no capital at the start of a sentence and no
//! corrections, and an address or a password gets a keyboard for prose. Only
//! the application knows which box is which, and [`follow`] is where it says
//! so: `GpuiInputActivity.gpuiKeyboardHint` keeps the last thing said and
//! shapes the keyboard by it.
//!
//! Off Android there is no keyboard to tell and all of this does nothing;
//! [`keyboard_inset`] is zero, unless `GPUI_KEYBOARD=<height>` pretends that
//! a keyboard of that height is up, which is the only way to look at what an
//! interface does about one without a phone.

use std::cell::Cell;

use gpui_kit::component::input::InputEvent;
use gpui_kit::prelude::*;
use gpui_kit::{App, Div, Entity, EntityId, Focusable, Pixels, Window, div, px};

/// What a box is for. The numbers are the ones `gpuiKeyboardHint` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// Nothing in particular: the plain keyboard.
    Plain = 0,
    /// Prose: sentences, capitals, corrections.
    Message = 1,
    /// An address: a URL, a host.
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
/// anything, given who spoke last. Kept apart from the telling so that it can
/// be tested where there is no keyboard.
///
/// A box says what it is for when it gets the focus and takes it back when it
/// loses it. The two do not come in a fixed order when the focus goes from
/// one box to another, so who spoke last is remembered: a box that lost the
/// focus only takes back what is still its own.
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
/// something, from the subscription to its `InputState`:
///
/// ```ignore
/// cx.subscribe(&state, |_, state, event, _| {
///     keyboard::follow(state.entity_id(), event, Purpose::Message)
/// })
/// ```
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
    let told = crate::android::with_activity(|env, activity| {
        env.call_method(
            activity,
            jni::jni_str!("gpuiKeyboardHint"),
            jni::jni_sig!("(I)V"),
            &[jni::objects::JValue::Int(purpose as i32)],
        )
        .map(drop)
    });
    if let Err(error) = told {
        tracing::warn!(%error, ?purpose, "could not tell the keyboard what a box is for");
    }
}

/// `input` (an `Input` or a `Textarea` over `state`), raising the on-screen
/// keyboard when it is tapped. The platform raises it only as an input gains
/// focus: one that kept its focus after the keyboard was put away would get
/// no keyboard for a tap.
pub fn on_tap<S: Focusable>(input: impl IntoElement, state: &Entity<S>, cx: &App) -> Div {
    let focus = state.read(cx).focus_handle(cx);
    let element = div().w_full().child(input);
    #[cfg(target_os = "android")]
    let element = element.capture_any_mouse_down(move |_, window, _| {
        // An input gaining focus from this tap raises it the platform's way.
        if focus.is_focused(window) && !is_up(window) {
            gpui_mobile::show_keyboard();
        }
    });
    #[cfg(not(target_os = "android"))]
    let _ = focus;
    element
}

/// How tall the on-screen keyboard is over `window`, or nothing while it is
/// down.
///
/// The platform reports one inset for whatever covers the bottom, the
/// navigation bar or the keyboard; one too tall to be the bar is the keyboard.
/// (`gpui_mobile::keyboard_height` is kept only on iOS.)
pub fn keyboard_inset(window: &Window) -> Pixels {
    let (_, bottom) = crate::insets::system_bars();
    #[cfg(not(target_os = "android"))]
    let bottom = bottom.max(pretended());
    if bottom > window.viewport_size().height * 0.2 {
        bottom
    } else {
        px(0.)
    }
}

/// Whether the keyboard is up over `window`.
pub fn is_up(window: &Window) -> bool {
    keyboard_inset(window) > px(0.)
}

#[cfg(not(target_os = "android"))]
fn pretended() -> Pixels {
    px(std::env::var("GPUI_KEYBOARD")
        .ok()
        .and_then(|height| height.parse().ok())
        .unwrap_or(0.))
}

#[cfg(test)]
mod tests {
    use gpui_kit::EntityId;
    use gpui_kit::component::input::InputEvent;

    use super::{Purpose, told};

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
        assert_eq!(told(Some(id(2)), id(1), &InputEvent::Blur, Purpose::Message), None);
    }

    #[test]
    fn typing_says_nothing() {
        assert_eq!(told(Some(id(1)), id(1), &InputEvent::Change, Purpose::Message), None);
    }
}
