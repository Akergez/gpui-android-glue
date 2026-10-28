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
//!   layers the system's own animation runs and leaves the application. The
//!   library's manifest turns predictive back on
//!   (`android:enableOnBackInvokedCallback`).
//! - On a desktop the same layers take Escape.
//!
//! With the on-screen keyboard up, back puts it away and does nothing else,
//! as everywhere on Android: the keyboard takes the key itself, and Escape
//! that reaches the application anyway while it is up (a gesture the
//! application was given) only puts it away.
//!
//! A layer pushed with [`push_over_widgets`] comes before the toolkit's: a
//! picture over the window outranks the input under it, which would
//! otherwise take Escape for its own. An open dialog still comes first.
//!
//! A toolkit dialog is not a layer by itself, and with predictive back the
//! gesture over a dialog with no layer under it would leave the application
//! with the dialog open. [`open_dialog`] opens one as a layer of its own.
//!
//! # The preview
//!
//! On Android 14 and later the gesture of predictive back is followed as it
//! goes: the page being left moves aside with the finger and shows the one
//! back goes to, and lifting the finger before the end leaves it where it
//! was. [`Guard::gesture`] says how far a gesture that would go to that
//! layer has gone, and [`preview`] lays a page out over the one under it by
//! that. A [`preview`] made with [`Preview::swipe_back`] also goes back by a
//! finger drawn across it to the right, anywhere, not only from the edge,
//! with the same preview; the system's gesture stays as it is. The swipe
//! moves the page by exactly as much as the finger has moved, pixel for
//! pixel, and only once it is lifted does the page glide on by itself.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::dialog::Dialog;
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Bounds, DispatchPhase, KeystrokeEvent, Pixels, ScrollDelta, ScrollWheelEvent, TouchPhase, Window,
    canvas, div, relative,
};

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

    /// The layer back goes to first: the topmost over the widgets, else the
    /// topmost under them.
    fn first(&self) -> Option<u64> {
        let topmost = |rank| self.layers.iter().rev().find(|(_, each, _)| *each == rank).map(|(id, _, _)| *id);
        topmost(Rank::OverWidgets).or_else(|| topmost(Rank::UnderWidgets))
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

impl Guard {
    /// The gesture of back under way, if back would go to this layer: none
    /// with a dialog open or another layer first. Read it in `render` and lay
    /// the page out by it, with [`preview`]; the window is drawn again for as
    /// long as it moves.
    pub fn gesture(&self, window: &mut Window, cx: &mut App) -> Option<Gesture> {
        if LAYERS.with(|layers| layers.borrow().first()) != Some(self.0) || dialog_open(window, cx) {
            return None;
        }
        let now = cx.background_executor().now();
        MOTION.with_borrow_mut(|motion| {
            let motion = motion.as_mut()?;
            motion.shown = true;
            let progress = match &motion.settle {
                Some(settle) => {
                    window.request_animation_frame();
                    settle.at(now)
                }
                None => motion.progress,
            };
            Some(Gesture { progress, from: motion.from })
        })
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

/// `window.open_dialog`, with the dialog a layer for as long as it is open:
/// back closes it even where nothing under it is a layer, and its gesture
/// is no page's.
pub fn open_dialog<F>(window: &mut Window, cx: &mut App, build: F)
where
    F: Fn(Dialog, &mut Window, &mut App) -> Dialog + 'static,
{
    // The toolkit's dialog takes Escape itself; the layer is for the gesture
    // to come here at all, and closes it if nothing else did.
    let guard = push(|window, cx| {
        let open = window.has_active_dialog(cx);
        window.close_dialog(cx);
        open
    });
    // The builder is dropped with the dialog, and the layer with it.
    window.open_dialog(cx, move |dialog, window, cx| {
        let _ = &guard;
        build(dialog, window, cx)
    });
}

/// Back, now, as if it had been pressed: what a button with an arrow does.
pub fn go_back(window: &mut Window, cx: &mut App) {
    window.dispatch_keystroke(escape(), cx);
}

pub(crate) fn init(cx: &mut App) {
    cx.intercept_keystrokes(|event, window, cx| {
        if !is_back(event) {
            return;
        }
        if crate::keyboard::is_up(window) {
            put_the_keyboard_away();
            if !SWIPED.get() {
                cx.stop_propagation();
                return;
            }
        }
        if !dialog_open(window, cx) && take_back(Rank::OverWidgets, window, cx) {
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
        let signals = signals().1.clone();
        cx.spawn(async move |cx| {
            while let Ok(signal) = signals.recv().await {
                cx.update(|cx| {
                    let window = cx.active_window().or_else(|| cx.windows().first().copied());
                    if let Some(window) = window {
                        let _ = window.update(cx, |_, window, cx| from_the_system(signal, window, cx));
                    }
                });
            }
        })
        .detach();
    }
}

/// Back with the keyboard up. The input keeps its focus; a tap raises the
/// keyboard again (`keyboard::on_tap`).
fn put_the_keyboard_away() {
    #[cfg(target_os = "android")]
    gpui_mobile::hide_keyboard();
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

// The gesture: the system's, or a swipe.

/// A gesture of back under way: see [`Guard::gesture`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gesture {
    /// How far it has gone: 0 where it started, 1 where the page is gone.
    pub progress: f32,
    /// The side of the screen it came from; the page moves away from it.
    pub from: Edge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
}

impl Gesture {
    /// How far the page has moved, as a share of its width: rightwards for a
    /// gesture from the left, leftwards from the right.
    pub fn offset(&self) -> f32 {
        let progress = self.progress.clamp(0., 1.);
        match self.from {
            Edge::Left => progress,
            Edge::Right => -progress,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    /// The system's predictive back: back is Escape, as for its key.
    System,
    /// A swipe across a [`Preview`]: back even with the keyboard up.
    Swipe,
}

/// The gesture under way, from its start until the page has settled.
struct Motion {
    /// Tells this gesture's settling from a later one's.
    id: u64,
    source: Source,
    from: Edge,
    progress: f32,
    /// After the finger is lifted: where the page goes.
    settle: Option<Settle>,
    /// Whether anyone drew a preview of it; if not, back goes at once.
    shown: bool,
}

/// The page on its way from where the finger left it to where it rests.
#[derive(Clone, Copy, Debug)]
struct Settle {
    from: f32,
    to: f32,
    start: Instant,
    duration: Duration,
}

/// From one end of the page to the other: a part of the way takes a part of
/// it.
const SETTLE_WHOLE: Duration = Duration::from_millis(260);

impl Settle {
    fn new(from: f32, to: f32, start: Instant) -> Self {
        Settle { from, to, start, duration: SETTLE_WHOLE.mul_f32((to - from).abs().min(1.)) }
    }

    /// Where the page is at `now`: quick at first, slowing to a stop.
    fn at(&self, now: Instant) -> f32 {
        if self.duration.is_zero() {
            return self.to;
        }
        let t = (now.saturating_duration_since(self.start).as_secs_f32() / self.duration.as_secs_f32()).min(1.);
        let eased = 1. - (1. - t).powi(3);
        self.from + (self.to - self.from) * eased
    }
}

thread_local! {
    static MOTION: RefCell<Option<Motion>> = const { RefCell::new(None) };
    static NEXT_MOTION: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    /// Set while a swipe's back is dispatched: it goes past the keyboard.
    static SWIPED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn begin(source: Source, from: Edge, progress: f32, window: &mut Window) {
    let id = NEXT_MOTION.get() + 1;
    NEXT_MOTION.set(id);
    MOTION.set(Some(Motion { id, source, from, progress: progress.clamp(0., 1.), settle: None, shown: false }));
    window.refresh();
}

fn follow(progress: f32, window: &mut Window) {
    MOTION.with_borrow_mut(|motion| {
        if let Some(motion) = motion.as_mut().filter(|motion| motion.settle.is_none()) {
            motion.progress = progress.clamp(0., 1.);
        }
    });
    window.refresh();
}

/// The finger is lifted: back if `commit`, else the page goes back where it
/// was. The page settles first, if it was shown.
fn release(commit: bool, window: &mut Window, cx: &mut App) {
    let now = cx.background_executor().now();
    let settling = MOTION.with_borrow_mut(|motion| {
        let motion = motion.as_mut()?;
        let from = motion.settle.map_or(motion.progress, |settle| settle.at(now));
        let settle = Settle::new(from, if commit { 1. } else { 0. }, now);
        motion.settle = Some(settle);
        Some((motion.id, motion.source, motion.shown && !cx.reduce_motion(), settle.duration))
    });
    let Some((id, source, shown, duration)) = settling else {
        // The three-button bar's back, and the old systems' gesture: no motion.
        if commit {
            go_back(window, cx);
        }
        return;
    };
    if !shown {
        settled(id, commit, source, window, cx);
        return;
    }
    window.refresh();
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(duration).await;
            let _ = cx.update(|window, cx| settled(id, commit, source, window, cx));
        })
        .detach();
}

/// The page has settled: the gesture is over, and back goes if it was meant
/// to.
fn settled(id: u64, commit: bool, source: Source, window: &mut Window, cx: &mut App) {
    let ours = MOTION.with_borrow_mut(|motion| {
        let ours = motion.as_ref().is_some_and(|motion| motion.id == id);
        if ours {
            *motion = None;
        }
        ours
    });
    if !ours {
        return;
    }
    window.refresh();
    if commit {
        match source {
            Source::System => go_back(window, cx),
            Source::Swipe => {
                SWIPED.set(true);
                go_back(window, cx);
                SWIPED.set(false);
            }
        }
    }
}

/// What `GpuiBack.java` says of the system's gesture.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
enum Signal {
    Started(Edge, f32),
    Progressed(f32),
    Cancelled,
    Invoked,
}

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn from_the_system(signal: Signal, window: &mut Window, cx: &mut App) {
    match signal {
        Signal::Started(from, progress) => begin(Source::System, from, progress, window),
        Signal::Progressed(progress) => follow(progress, window),
        Signal::Cancelled => release(false, window, cx),
        Signal::Invoked => release(true, window, cx),
    }
}

// The swipe.

/// A finger drawn to the right across a [`Preview`] made with
/// [`Preview::swipe_back`]. gpui makes a finger's movement a pan, scroll
/// events along one axis; one to the right is taken here, before anything
/// under it scrolls, and followed until it is lifted.
#[derive(Default)]
enum Swipe {
    #[default]
    Idle,
    Following {
        width: Pixels,
        travelled: Pixels,
        /// The last moments' travel, for the speed at release.
        samples: VecDeque<(Instant, f32)>,
    },
    /// Lifted at speed: gpui flings the pan on, and the fling is the swipe's.
    Spent { until: Instant },
}

thread_local! {
    static SWIPE: RefCell<Swipe> = const { RefCell::new(Swipe::Idle) };
}

/// How long a fling after a swipe can last, at most.
const FLING_AT_MOST: Duration = Duration::from_secs(3);
/// The moments the speed at release is taken over.
const SPEED_WINDOW: Duration = Duration::from_millis(100);

/// Whether a swipe lifted at `progress`, moving at `speed` (pixels a second,
/// rightwards; none if it cannot be told), goes back.
fn commits(progress: f32, speed: Option<f32>) -> bool {
    match speed {
        Some(speed) if speed > 700. => true,
        Some(speed) if speed < -300. => false,
        _ => progress >= 0.4,
    }
}

/// The speed over the last samples: none for samples all at one moment, as a
/// script's.
fn speed(samples: &VecDeque<(Instant, f32)>) -> Option<f32> {
    let (first, last) = (samples.front()?, samples.back()?);
    let seconds = last.0.saturating_duration_since(first.0).as_secs_f32();
    (seconds > 0.005).then(|| (last.1 - first.1) / seconds)
}

/// One scroll event, in the capture phase, over a swipe area of `bounds`.
/// Answers whether it was the swipe's.
fn on_scroll(event: &ScrollWheelEvent, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) -> bool {
    let ScrollDelta::Pixels(delta) = event.delta else { return false };
    let now = cx.background_executor().now();
    let mut swipe = SWIPE.take();
    if let Swipe::Spent { until } = swipe
        && (event.touch_phase == TouchPhase::Started || now > until)
    {
        swipe = Swipe::Idle;
    }
    let (next, ours, released) = match swipe {
        Swipe::Idle => {
            let starts = event.touch_phase == TouchPhase::Started
                && delta.x > Pixels::ZERO
                && delta.y == Pixels::ZERO
                && bounds.contains(&event.position)
                && bounds.size.width > Pixels::ZERO
                && MOTION.with_borrow(Option::is_none)
                && LAYERS.with(|layers| layers.borrow().first()).is_some()
                && !dialog_open(window, cx);
            if starts {
                let width = bounds.size.width;
                begin(Source::Swipe, Edge::Left, delta.x / width, window);
                let samples = VecDeque::from([(now, f32::from(delta.x))]);
                (Swipe::Following { width, travelled: delta.x, samples }, true, None)
            } else {
                (Swipe::Idle, false, None)
            }
        }
        Swipe::Following { width, travelled, mut samples } => {
            let travelled = (travelled + delta.x).max(Pixels::ZERO);
            samples.push_back((now, f32::from(travelled)));
            while samples.len() > 2 && samples.front().is_some_and(|(at, _)| now.duration_since(*at) > SPEED_WINDOW) {
                samples.pop_front();
            }
            let progress = travelled / width;
            match event.touch_phase {
                TouchPhase::Ended => {
                    follow(progress, window);
                    (Swipe::Spent { until: now + FLING_AT_MOST }, true, Some(commits(progress, speed(&samples))))
                }
                TouchPhase::Cancelled => (Swipe::Idle, true, Some(false)),
                _ => {
                    follow(progress, window);
                    (Swipe::Following { width, travelled, samples }, true, None)
                }
            }
        }
        Swipe::Spent { until } => {
            let next = if event.touch_phase == TouchPhase::Ended { Swipe::Idle } else { Swipe::Spent { until } };
            (next, true, None)
        }
    };
    SWIPE.set(next);
    if let Some(commit) = released {
        release(commit, window, cx);
    }
    ours
}

/// Listens for the swipe over the area it is laid over.
fn swipe_area() -> impl IntoElement {
    canvas(
        |_, _, _| (),
        |bounds, (), window, _| {
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture && on_scroll(event, bounds, window, cx) {
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .size_full()
}

// The preview.

/// A page laid out over the one back goes to; see [`preview`].
pub struct Preview {
    top: AnyElement,
    under: Option<AnyElement>,
    gesture: Option<Gesture>,
    swipe: bool,
}

/// `top` — the whole page, opaque, filling what it is given — laid out by
/// the gesture under way (from [`Guard::gesture`]): moved aside, over
/// `under` shaded. Without a gesture `top` is all there is; build `under`
/// only when there is one. The page keeps its place in the element tree
/// either way, and what scrolls in it its scroll.
pub fn preview(top: impl IntoElement, gesture: Option<Gesture>) -> Preview {
    Preview { top: top.into_any_element(), under: None, gesture, swipe: false }
}

/// How far the page under moves along with the one over it, as a share of
/// its width.
const UNDER_LAGS: f32 = 0.25;
/// How dark the page under is at the start.
const UNDER_SHADE: f32 = 0.3;

impl Preview {
    /// What back goes to, drawn under the page while a gesture moves it.
    pub fn under(mut self, under: Option<AnyElement>) -> Self {
        self.under = under;
        self
    }

    /// Back by a finger drawn to the right anywhere across the page (on a
    /// desktop, two fingers on a touchpad), as long as back would go to a
    /// layer. Whatever scrolls sideways in the page loses that direction to
    /// it: keep it to pages that have none.
    pub fn swipe_back(mut self, swipe: bool) -> Self {
        self.swipe = swipe;
        self
    }
}

impl IntoElement for Preview {
    type Element = AnyElement;

    fn into_element(self) -> AnyElement {
        let offset = self.gesture.map_or(0., |gesture| gesture.offset());
        let progress = offset.abs();
        let under = self.under.filter(|_| self.gesture.is_some()).map(|under| {
            let shade = gpui_kit::black().opacity(UNDER_SHADE * (1. - progress));
            div()
                .id("back-preview-under")
                .absolute()
                .top_0()
                .bottom_0()
                .w_full()
                .left(relative(-offset.signum() * UNDER_LAGS * (1. - progress)))
                .flex()
                .flex_col()
                .child(under)
                .child(div().absolute().size_full().bg(shade))
        });
        div()
            .relative()
            .size_full()
            .overflow_hidden()
            .children(under)
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .w_full()
                    .left(relative(offset))
                    .flex()
                    .flex_col()
                    .when(progress > 0., |page| page.shadow_lg())
                    .child(self.top)
                    .when(self.swipe, |page| page.child(swipe_area())),
            )
            .into_any_element()
    }
}

// From the Java side.

/// What `GpuiBack` says, from the system's UI thread to the application's.
#[cfg(target_os = "android")]
fn signals() -> &'static (async_channel::Sender<Signal>, async_channel::Receiver<Signal>) {
    static SIGNALS: std::sync::OnceLock<(async_channel::Sender<Signal>, async_channel::Receiver<Signal>)> =
        std::sync::OnceLock::new();
    SIGNALS.get_or_init(async_channel::unbounded)
}

/// `GpuiBack.nativeBack`: back, by the gesture or by the three-button bar.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_dev_gpui_glue_GpuiBack_nativeBack(_env: *mut std::ffi::c_void, _class: *mut std::ffi::c_void) {
    let _ = signals().0.try_send(Signal::Invoked);
}

/// `GpuiBack.nativeBackStarted`: the gesture has begun, from `edge`
/// (`BackEvent.EDGE_RIGHT` is 1).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_dev_gpui_glue_GpuiBack_nativeBackStarted(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    edge: i32,
    progress: f32,
) {
    let from = if edge == 1 { Edge::Right } else { Edge::Left };
    let _ = signals().0.try_send(Signal::Started(from, progress));
}

/// `GpuiBack.nativeBackProgressed`.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_dev_gpui_glue_GpuiBack_nativeBackProgressed(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
    progress: f32,
) {
    let _ = signals().0.try_send(Signal::Progressed(progress));
}

/// `GpuiBack.nativeBackCancelled`: the finger went back where it started.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "C" fn Java_dev_gpui_glue_GpuiBack_nativeBackCancelled(
    _env: *mut std::ffi::c_void,
    _class: *mut std::ffi::c_void,
) {
    let _ = signals().0.try_send(Signal::Cancelled);
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    use super::{Edge, Gesture, Layers, Rank, Settle, commits, speed};

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

    #[test]
    fn back_goes_first_over_the_widgets() {
        let mut layers = Layers::new();
        let picture = layers.push(Rank::OverWidgets, "picture");
        let page = layers.push(Rank::UnderWidgets, "page");
        assert_eq!(layers.first(), Some(picture));
        layers.remove(picture);
        assert_eq!(layers.first(), Some(page));
        layers.remove(page);
        assert_eq!(layers.first(), None);
    }

    #[test]
    fn the_page_moves_away_from_the_edge() {
        assert_eq!(Gesture { progress: 0.25, from: Edge::Left }.offset(), 0.25);
        assert_eq!(Gesture { progress: 0.25, from: Edge::Right }.offset(), -0.25);
        assert_eq!(Gesture { progress: 1.5, from: Edge::Left }.offset(), 1.);
    }

    #[test]
    fn settling_goes_the_whole_way_and_stops() {
        let start = Instant::now();
        let settle = Settle::new(0.5, 1., start);
        assert!(settle.duration.abs_diff(super::SETTLE_WHOLE / 2) < Duration::from_millis(1));
        assert_eq!(settle.at(start), 0.5);
        let halfway = settle.at(start + settle.duration / 2);
        assert!(halfway > 0.75 && halfway < 1., "{halfway}");
        assert_eq!(settle.at(start + Duration::from_secs(5)), 1.);
    }

    #[test]
    fn settling_nowhere_is_done_at_once() {
        let start = Instant::now();
        assert_eq!(Settle::new(0., 0., start).at(start), 0.);
    }

    #[test]
    fn a_swipe_goes_back_far_enough_or_fast_enough() {
        assert!(commits(0.5, None));
        assert!(!commits(0.2, None));
        assert!(commits(0.1, Some(1200.)));
        assert!(!commits(0.8, Some(-800.)));
        assert!(!commits(0.3, Some(100.)));
    }

    #[test]
    fn speed_is_over_the_samples() {
        let start = Instant::now();
        let samples = VecDeque::from([(start, 0.), (start + Duration::from_millis(50), 50.)]);
        assert_eq!(speed(&samples), Some(1000.));
        // A script's swipe: every sample at one moment.
        assert_eq!(speed(&VecDeque::from([(start, 0.), (start, 300.)])), None);
    }
}

#[cfg(test)]
mod in_a_window {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    use gpui::{
        AnyWindowHandle, AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, ParentElement as _,
        PlatformInput, Render, StatefulInteractiveElement as _, Styled as _, TestAppContext, TouchEvent, TouchId,
        TouchPhase, Window, WindowBounds, WindowOptions, div, point, px,
    };
    use gpui_kit::base::InteractiveElementExt as _;
    use gpui_kit::component::WindowExt as _;
    use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};

    use super::{Guard, any, open_dialog, preview};

    /// A page that goes back to a list: a layer for as long as it is open.
    struct Screen {
        page: Option<Guard>,
    }

    impl Render for Screen {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let gesture = self.page.as_ref().and_then(|page| page.gesture(window, cx));
            let list = div().size_full().child("the list").into_any_element();
            let page = div()
                .id("page")
                .size_full()
                .overflow_y_scroll()
                .lock_scroll_axis()
                .children((0..50).map(|line| div().h(px(40.)).child(format!("line {line}"))))
                .test_support();
            preview(page, gesture).under(gesture.map(|_| list)).swipe_back(true)
        }
    }

    fn screen(cx: &mut TestAppContext) -> (AnyWindowHandle, Rc<Cell<u32>>) {
        let backs = Rc::new(Cell::new(0));
        let counted = backs.clone();
        let window = cx.update(|cx| {
            gpui_kit::component::init(cx);
            super::init(cx);
            let bounds = Bounds::maximized(None, cx);
            let (window, _) = gpui_kit::open_window(
                WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
                cx,
                move |_, cx| {
                    cx.new(|cx| {
                        let screen = cx.weak_entity();
                        let counted = counted.clone();
                        let page = super::push(move |_, cx| {
                            counted.set(counted.get() + 1);
                            screen.update(cx, |screen: &mut Screen, cx| {
                                screen.page = None;
                                cx.notify();
                            })
                            .is_ok()
                        });
                        Screen { page: Some(page) }
                    })
                },
            )
            .expect("a test window");
            window
        });
        cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
        (window, backs)
    }

    /// A finger from (100, 200) moved by (dx, dy) in steps a frame apart,
    /// and lifted.
    fn swipe(cx: &mut TestAppContext, window: AnyWindowHandle, dx: f32, dy: f32) {
        const MOVES: u16 = 8;
        for step in 0..=MOVES + 1 {
            let (phase, done) = match step {
                0 => (TouchPhase::Started, 0.),
                step if step <= MOVES => (TouchPhase::Moved, f32::from(step) / f32::from(MOVES)),
                _ => (TouchPhase::Ended, 1.),
            };
            let position = point(px(100. + dx * done), px(200. + dy * done));
            let touch = TouchEvent { id: TouchId(1), phase, position, predicted_position: None, force: None };
            cx.update_window(window, |_, window, cx| {
                window.dispatch_event(PlatformInput::Touch(touch), cx);
                window.render_frame(cx);
            })
            .unwrap();
            cx.executor().advance_clock(Duration::from_millis(30));
        }
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
    }

    #[gpui::test]
    fn a_swipe_to_the_right_goes_back(cx: &mut TestAppContext) {
        let (window, backs) = screen(cx);
        swipe(cx, window, 300., 0.);
        assert_eq!(backs.get(), 1);
        assert!(!any());
    }

    #[gpui::test]
    fn the_page_follows_the_finger_one_to_one(cx: &mut TestAppContext) {
        let (window, backs) = screen(cx);
        let page_left = |cx: &mut TestAppContext| {
            cx.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                f32::from(window.find("page").bounds().left())
            })
            .unwrap()
        };
        let start = page_left(cx);
        let touch = |phase, x: f32| TouchEvent {
            id: TouchId(1),
            phase,
            position: point(px(100. + x), px(200.)),
            predicted_position: None,
            force: None,
        };
        let touched = |cx: &mut TestAppContext, event| {
            cx.update_window(window, |_, window, cx| window.dispatch_event(PlatformInput::Touch(event), cx)).unwrap();
            cx.executor().advance_clock(Duration::from_millis(16));
        };
        touched(cx, touch(TouchPhase::Started, 0.));
        for x in [20., 57., 130., 211., 90.] {
            touched(cx, touch(TouchPhase::Moved, x));
            assert_eq!(page_left(cx) - start, x, "the page is where the finger is");
        }
        // Lifted a third of the way, slowly: the page goes back where it was.
        cx.executor().advance_clock(Duration::from_millis(300));
        touched(cx, touch(TouchPhase::Ended, 90.));
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert_eq!(page_left(cx), start);
        assert_eq!(backs.get(), 0);
    }

    #[gpui::test]
    fn a_swipe_up_scrolls_and_does_not_go_back(cx: &mut TestAppContext) {
        let (window, backs) = screen(cx);
        swipe(cx, window, 0., -300.);
        assert_eq!(backs.get(), 0);
        assert!(any());
    }

    #[gpui::test]
    fn a_swipe_to_the_left_does_not_go_back(cx: &mut TestAppContext) {
        let (window, backs) = screen(cx);
        swipe(cx, window, -300., 0.);
        assert_eq!(backs.get(), 0);
    }

    #[gpui::test]
    fn a_dialog_is_a_layer_while_it_is_open(cx: &mut TestAppContext) {
        let (window, backs) = screen(cx);
        cx.update_window(window, |_, window, cx| open_dialog(window, cx, |dialog, _, _| dialog.title("A dialog")))
            .unwrap();
        cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
        // Over a dialog the page takes no swipe, and back closes the dialog.
        swipe(cx, window, 300., 0.);
        assert_eq!(backs.get(), 0);
        cx.update_window(window, |_, window, cx| super::go_back(window, cx)).unwrap();
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
        assert!(!cx.update_window(window, |_, window, cx| window.has_active_dialog(cx)).unwrap());
        assert_eq!(backs.get(), 0);
        // Only the page's layer is left.
        cx.update_window(window, |_, window, cx| super::go_back(window, cx)).unwrap();
        assert_eq!(backs.get(), 1);
        assert!(!any());
    }
}
