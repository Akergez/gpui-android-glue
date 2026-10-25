//! The system's back, as a stack of things to take back.
//!
//! Whatever back should close — a page over a list, a panel, a picture over
//! the window — says so while it is open: [`push`] a handler and keep the
//! [`Guard`]; dropping it takes the layer away. Back goes to the topmost
//! layer, and down the stack for as long as handlers answer `false` (nothing
//! to take back after all). With nothing left, the application goes under the
//! home screen, as any Android application does.
//!
//! Back is Escape, everywhere:
//!
//! - gpui-mobile hands the back key to the window as the keystroke `escape`,
//!   so the toolkit's own uses of Escape — a dialog, a menu, a completion
//!   list, an input dropping its selection — come first, as they do on a
//!   desktop. A layer gets back only if nothing else wanted it.
//! - The gesture of predictive back is no key: `GpuiBack.java` takes it while
//!   there is a layer, and it is turned into the same keystroke here. With no
//!   layers the system's own animation runs and leaves the application.
//! - On a desktop the same layers take Escape.
//!
//! A layer pushed with [`push_over_widgets`] comes before the toolkit's: a
//! picture over the window outranks the input under it, which would
//! otherwise take Escape for its own. An open dialog still comes first.
//!
//! A toolkit dialog is not a layer by itself. Where predictive back is on and
//! a dialog may be open over no layer, push one for it, or the gesture leaves
//! the application with the dialog open.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::component::WindowExt as _;
use gpui_kit::{App, KeystrokeEvent, Window};

type Handler = Rc<dyn Fn(&mut Window, &mut App) -> bool>;

/// Where a layer stands against the toolkit's own uses of Escape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rank {
    /// After them: only an Escape nothing else wanted.
    UnderWidgets,
    /// Before them, a dialog excepted.
    OverWidgets,
}

/// The layers, oldest first. Kept apart from the window so that what goes
/// where can be tested without one.
struct Layers<H> {
    next: u64,
    layers: Vec<(u64, Rank, H)>,
}

impl<H: Clone> Layers<H> {
    const fn new() -> Self {
        Layers { next: 0, layers: Vec::new() }
    }

    fn push(&mut self, rank: Rank, handler: H) -> u64 {
        self.next += 1;
        self.layers.push((self.next, rank, handler));
        self.next
    }

    fn remove(&mut self, id: u64) {
        self.layers.retain(|(each, _, _)| *each != id);
    }

    fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// The handlers of `rank` to try, topmost first. Copied out, so that a
    /// handler may push and drop layers as it runs.
    fn to_try(&self, rank: Rank) -> Vec<H> {
        self.layers
            .iter()
            .rev()
            .filter(|(_, each, _)| *each == rank)
            .map(|(_, _, handler)| handler.clone())
            .collect()
    }
}

thread_local! {
    static LAYERS: RefCell<Layers<Handler>> = const { RefCell::new(Layers::new()) };
}

/// A layer's place on the stack, for as long as it is kept.
#[must_use = "the layer is taken away when the guard is dropped"]
pub struct Guard(u64);

impl Drop for Guard {
    fn drop(&mut self) {
        // The stack may be gone already, at the end of the thread.
        if LAYERS.try_with(|layers| layers.borrow_mut().remove(self.0)).is_ok() {
            tell_the_system();
        }
    }
}

/// Puts a layer on top: `handler` takes back what it opened and answers
/// whether there was anything to take back.
pub fn push(handler: impl Fn(&mut Window, &mut App) -> bool + 'static) -> Guard {
    push_ranked(Rank::UnderWidgets, Rc::new(handler))
}

/// Like [`push`], but before the toolkit's own uses of Escape: see the
/// module's documentation.
pub fn push_over_widgets(handler: impl Fn(&mut Window, &mut App) -> bool + 'static) -> Guard {
    push_ranked(Rank::OverWidgets, Rc::new(handler))
}

fn push_ranked(rank: Rank, handler: Handler) -> Guard {
    let id = LAYERS.with(|layers| layers.borrow_mut().push(rank, handler));
    tell_the_system();
    Guard(id)
}

/// Whether anything is on the stack.
pub fn any() -> bool {
    LAYERS.with(|layers| !layers.borrow().is_empty())
}

/// Back, now, as if it had been pressed: what a button with an arrow does.
pub fn go_back(window: &mut Window, cx: &mut App) {
    window.dispatch_keystroke(escape(), cx);
}

pub(crate) fn init(cx: &mut App) {
    cx.intercept_keystrokes(|event, window, cx| {
        if is_back(event) && !dialog_open(window, cx) && take_back(Rank::OverWidgets, window, cx) {
            cx.stop_propagation();
        }
    })
    .detach();
    // Observers hear a keystroke after it is dispatched, with the action that
    // took it, if one did.
    cx.observe_keystrokes(|event, window, cx| {
        if is_back(event) && event.action.is_none() && !take_back(Rank::UnderWidgets, window, cx) {
            nothing_to_take_back();
        }
    })
    .detach();

    #[cfg(target_os = "android")]
    {
        let gestures = gestures().1.clone();
        cx.spawn(async move |cx| {
            while gestures.recv().await.is_ok() {
                cx.update(|cx| {
                    let window = cx.active_window().or_else(|| cx.windows().first().copied());
                    if let Some(window) = window {
                        let _ = window.update(cx, |_, window, cx| go_back(window, cx));
                    }
                });
            }
        })
        .detach();
    }
}

fn escape() -> gpui_kit::Keystroke {
    gpui_kit::Keystroke::parse("escape").expect("escape is a keystroke")
}

/// Whether a toolkit dialog is open in `window`. Only a window whose root is
/// the toolkit's `Root` can have one, and asking any other panics.
fn dialog_open(window: &mut Window, cx: &mut App) -> bool {
    matches!(window.root::<gpui_kit::base::Root>(), Some(Some(_))) && window.has_active_dialog(cx)
}

fn is_back(event: &KeystrokeEvent) -> bool {
    event.keystroke.key == "escape" && !event.keystroke.modifiers.modified()
}

/// Tries the layers of `rank`, topmost first, until one takes back.
fn take_back(rank: Rank, window: &mut Window, cx: &mut App) -> bool {
    let handlers = LAYERS.with(|layers| layers.borrow().to_try(rank));
    handlers.into_iter().any(|handler| handler(window, cx))
}

/// Back with nothing to take back: what the system does for an application
/// that does not take it, leave it running under the home screen.
fn nothing_to_take_back() {
    #[cfg(target_os = "android")]
    {
        let done = crate::android::with_class("dev.gpui.glue.GpuiBack", |env, activity, class| {
            env.call_static_method(
                class,
                jni::jni_str!("moveToBack"),
                jni::jni_sig!("(Landroid/app/Activity;)V"),
                &[jni::objects::JValue::Object(activity)],
            )
            .map(drop)
        });
        if let Err(error) = done {
            tracing::warn!(%error, "could not go under the home screen");
        }
    }
}

/// Tells `GpuiBack.java` whether there is a layer, when that has changed: the
/// gesture of predictive back is the application's only while there is.
#[cfg(target_os = "android")]
fn tell_the_system() {
    thread_local! {
        /// What the Java side was last told.
        static TOLD: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    }
    let any = any();
    if TOLD.replace(Some(any)) == Some(any) {
        return;
    }
    let done = crate::android::with_class("dev.gpui.glue.GpuiBack", |env, activity, class| {
        env.call_static_method(
            class,
            jni::jni_str!("setEnabled"),
            jni::jni_sig!("(Landroid/app/Activity;Z)V"),
            &[jni::objects::JValue::Object(activity), jni::objects::JValue::Bool(any)],
        )
        .map(drop)
    });
    if let Err(error) = done {
        tracing::warn!(%error, "could not tell the system whether back is taken");
        // Told nothing: try again next time.
        TOLD.set(None);
    }
}

/// A desktop has no back but Escape, which is always the application's.
#[cfg(not(target_os = "android"))]
fn tell_the_system() {}

/// Gestures from `GpuiBack.nativeBack`, which runs on the system's UI thread,
/// to the application's.
#[cfg(target_os = "android")]
fn gestures() -> &'static (async_channel::Sender<()>, async_channel::Receiver<()>) {
    static GESTURES: std::sync::OnceLock<(async_channel::Sender<()>, async_channel::Receiver<()>)> =
        std::sync::OnceLock::new();
    GESTURES.get_or_init(async_channel::unbounded)
}

/// `GpuiBack.nativeBack`: the gesture of predictive back.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_dev_gpui_glue_GpuiBack_nativeBack(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    let _ = gestures().0.try_send(());
}

#[cfg(test)]
mod tests {
    use super::{Layers, Rank};

    #[test]
    fn the_topmost_layer_is_tried_first() {
        let mut layers = Layers::new();
        layers.push(Rank::UnderWidgets, "list");
        layers.push(Rank::UnderWidgets, "page");
        assert_eq!(layers.to_try(Rank::UnderWidgets), ["page", "list"]);
    }

    #[test]
    fn a_layer_taken_away_is_not_tried() {
        let mut layers = Layers::new();
        layers.push(Rank::UnderWidgets, "list");
        let page = layers.push(Rank::UnderWidgets, "page");
        layers.remove(page);
        assert_eq!(layers.to_try(Rank::UnderWidgets), ["list"]);
    }

    #[test]
    fn ranks_are_tried_apart() {
        let mut layers = Layers::new();
        layers.push(Rank::OverWidgets, "picture");
        layers.push(Rank::UnderWidgets, "page");
        assert_eq!(layers.to_try(Rank::OverWidgets), ["picture"]);
        assert_eq!(layers.to_try(Rank::UnderWidgets), ["page"]);
    }

    #[test]
    fn removing_one_layer_leaves_its_twin() {
        let mut layers = Layers::new();
        let first = layers.push(Rank::UnderWidgets, "panel");
        layers.push(Rank::UnderWidgets, "panel");
        layers.remove(first);
        assert!(!layers.is_empty());
    }
}
