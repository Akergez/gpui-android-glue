//! The system's menu over a selection made by touch — Cut, Copy, Paste,
//! Select all, in the phone's language and look — in place of the toolkit's.
//!
//! gpui-kit draws a menu of its own over what a long press selects, in an
//! input and in text, and nothing in it reaches the phone's clipboard. Here
//! every input the toolkit makes and every window's text selection is
//! followed from the start ([`crate::init`]): the moment the toolkit opens its
//! menu, it is closed again, before it is drawn, and `GpuiEditMenu.java`
//! opens the system's floating toolbar over the same place. Its commands are
//! carried out the toolkit's way — an input's own Cut, Copy and Paste
//! actions, its Select All — with the phone's clipboard brought in before a
//! paste and the copy handed out after ([`crate::clipboard`]). The handles at
//! the ends of the selection stay the toolkit's.
//!
//! The toolbar follows the selection as it scrolls, steps aside while a
//! handle is dragged and comes back after, and goes away as soon as the
//! selection is something else. A tap on the selection brings it back, as
//! the toolkit's own would come back.
//!
//! Off Android the toolkit's menu stays.

use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::base::input::{self, EditorMode, InputBaseState, InputMode, InputModeKind, TextareaMode};
use gpui_kit::base::{Root, TextSelection, TouchSelectionSnapshot};
use gpui_kit::{Action, AnyWindowHandle, App, Bounds, Context, EntityId, Focusable as _, Pixels, Window};

/// A command of the menu. The numbers are what `GpuiEditMenu` sends back;
/// `1 << (n - 1)` is the command's bit in what it is told to offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Cut = 1,
    Copy = 2,
    Paste = 3,
    SelectAll = 4,
}

impl Command {
    // What the toolbar sends exists only on Android; so does everything that
    // carries a command out.
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    fn from_code(code: i32) -> Option<Self> {
        [Command::Cut, Command::Copy, Command::Paste, Command::SelectAll]
            .into_iter()
            .find(|command| *command as i32 == code)
    }

    fn bit(self) -> i32 {
        1 << (self as i32 - 1)
    }
}

/// The commands to offer, as `GpuiEditMenu.show` takes them.
fn items(commands: impl IntoIterator<Item = Command>) -> i32 {
    commands.into_iter().fold(0, |items, command| items | command.bit())
}

/// Whose selection the menu is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Input(EntityId),
    Text(AnyWindowHandle),
}

/// How a selection made by touch stands, as far as the menu goes.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seen {
    /// The toolkit has opened its menu over it.
    menu_open: bool,
    /// One of its handles is being dragged.
    dragging: bool,
    /// Where its ends in view are; nothing when it is scrolled away.
    bounds: Option<Bounds<Pixels>>,
}

impl Seen {
    fn of(snapshot: TouchSelectionSnapshot) -> Self {
        Seen {
            menu_open: snapshot.is_menu_open(),
            dragging: snapshot.dragging().is_some(),
            bounds: snapshot.bounds(),
        }
    }
}

/// What the system's menu should do.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Step {
    /// Be over these bounds: opened, or moved there.
    Over(Bounds<Pixels>),
    Away,
    /// Stay as it is: open elsewhere, or not open.
    Leave,
}

/// What follows from `seen` (nothing: no selection by touch, or the owner
/// has lost the focus) for the menu, given whether it is over this owner
/// now. The toolkit's own menu is closed whenever it is open, whatever this
/// says.
///
/// The toolkit opens its menu when a long press ends, a drag of a handle
/// ends, a scroll under the finger ends, or the selection is tapped; once it
/// is closed again, the system's menu is what remains, and it follows the
/// selection until there is none.
fn step(seen: Option<Seen>, ours: bool) -> Step {
    match seen {
        Some(Seen { dragging: false, bounds: Some(bounds), menu_open }) if menu_open || ours => {
            Step::Over(bounds)
        }
        _ if ours => Step::Away,
        _ => Step::Leave,
    }
}

/// `bounds` in the window's physical pixels, as Android draws: left, top,
/// right, bottom.
fn physical(bounds: Bounds<Pixels>, scale: f32) -> [i32; 4] {
    [bounds.left(), bounds.top(), bounds.right(), bounds.bottom()]
        .map(|edge| (f32::from(edge) * scale).round() as i32)
}

type Run = Rc<dyn Fn(Command, &mut App)>;

/// The system's menu, while it is open.
struct Open {
    owner: Owner,
    at: [i32; 4],
    items: i32,
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    run: Run,
}

thread_local! {
    static OPEN: RefCell<Option<Open>> = const { RefCell::new(None) };
}

fn is_over(owner: Owner) -> bool {
    OPEN.with_borrow(|open| open.as_ref().is_some_and(|open| open.owner == owner))
}

/// Does `step` for `owner`'s selection.
fn follow(owner: Owner, step: Step, scale: f32, items: impl FnOnce() -> i32, run: impl FnOnce() -> Run) {
    match step {
        Step::Over(bounds) => {
            let at = physical(bounds, scale);
            let items = items();
            if items == 0 {
                return close(owner);
            }
            let same = OPEN.with_borrow(|open| {
                open.as_ref().is_some_and(|open| open.owner == owner && open.at == at && open.items == items)
            });
            if same {
                return;
            }
            OPEN.set(Some(Open { owner, at, items, run: run() }));
            system::show(at, items);
        }
        Step::Away => close(owner),
        Step::Leave => {}
    }
}

/// Puts the menu away if it is over `owner`.
fn close(owner: Owner) {
    if is_over(owner) {
        OPEN.set(None);
        system::hide();
    }
}

/// A command from the toolbar, or `0`: the system closed it.
#[cfg_attr(not(target_os = "android"), allow(dead_code))]
fn heard(code: i32, cx: &mut App) {
    let Some(command) = Command::from_code(code) else {
        // A toolbar opened since, before this was heard, is not known to be
        // wanted any more either: it goes too, and comes back when the
        // selection asks for it, rather than staying after it.
        OPEN.set(None);
        system::hide();
        return;
    };
    let run = OPEN.with_borrow(|open| open.as_ref().map(|open| open.run.clone()));
    if let Some(run) = run {
        run(command, cx);
    }
}

/// An input's selection. Called whenever the input changes.
fn follow_input<M: InputModeKind>(
    state: &mut InputBaseState<M>,
    window: &mut Window,
    cx: &mut Context<InputBaseState<M>>,
) {
    let owner = Owner::Input(cx.entity_id());
    let snapshot = state
        .touch_selection()
        .filter(|_| state.focus_handle(cx).is_focused(window));
    let seen = snapshot.map(Seen::of);
    let step = step(seen, is_over(owner));
    if seen.is_some_and(|seen| seen.menu_open) {
        state.close_edit_menu(cx);
    }
    opening(owner, step, cx);
    let items = || {
        // What the toolkit's menu would offer. Paste is offered when the text
        // can change; the toolbar leaves it out when the clipboard is empty.
        let capabilities = state.context_menu_capabilities();
        let (editable, copyable) = (capabilities.is_editable(), capabilities.is_copyable());
        let everything = state.selected_range() == (0..state.text().len());
        let mut commands = Vec::with_capacity(4);
        if editable && copyable {
            commands.push(Command::Cut);
        }
        if copyable {
            commands.push(Command::Copy);
        }
        if editable {
            commands.push(Command::Paste);
        }
        if state.text().len() > 0 && !everything {
            commands.push(Command::SelectAll);
        }
        items(commands)
    };
    let run = || {
        let input = cx.weak_entity();
        let focus = state.focus_handle(cx);
        let window = window.window_handle();
        Rc::new(move |command: Command, cx: &mut App| {
            let _ = window.update(cx, |_, window, cx| {
                let dispatch = |action: &dyn Action, window: &mut Window, cx: &mut App| {
                    focus.dispatch_action(action, window, cx)
                };
                match command {
                    Command::Cut => {
                        dispatch(&input::Cut, window, cx);
                        crate::clipboard::push(cx);
                    }
                    Command::Copy => {
                        dispatch(&input::Copy, window, cx);
                        crate::clipboard::push(cx);
                    }
                    Command::Paste => {
                        crate::clipboard::pull(cx);
                        dispatch(&input::Paste, window, cx);
                    }
                    Command::SelectAll => {
                        let _ = input.update(cx, |state, cx| state.select_all_from_edit_menu(window, cx));
                    }
                }
            });
        }) as Run
    };
    follow(owner, step, window.scale_factor(), items, run);
}

/// Before the toolbar opens: a copy made some other way goes to the phone's
/// clipboard, where the toolbar looks to offer Paste.
fn opening(owner: Owner, step: Step, cx: &mut App) {
    if matches!(step, Step::Over(_)) && !is_over(owner) {
        crate::clipboard::push(cx);
    }
}

/// Every input of the mode `M`, from the moment it is made.
fn watch_inputs<M: InputModeKind>(cx: &mut App) {
    cx.observe_new(|_: &mut InputBaseState<M>, window, cx| {
        let Some(window) = window else { return };
        let input = cx.entity();
        cx.observe_in(&input, window, |state, _, window, cx| follow_input(state, window, cx))
            .detach();
        let owner = Owner::Input(input.entity_id());
        cx.on_release(move |_, _| close(owner)).detach();
    })
    .detach();
}

/// A window's text selection (the one text views take part in). Called
/// whenever it changes.
fn follow_text(window: &mut Window, cx: &mut App) {
    let handle = window.window_handle();
    let owner = Owner::Text(handle);
    let seen = TextSelection::touch_selection(window, cx).map(Seen::of);
    let step = step(seen, is_over(owner));
    if seen.is_some_and(|seen| seen.menu_open) {
        TextSelection::close_edit_menu(window, cx);
    }
    opening(owner, step, cx);
    // Read-only text: what the toolkit's menu offers there.
    let items = || items([Command::Copy, Command::SelectAll]);
    let run = || {
        Rc::new(move |command: Command, cx: &mut App| {
            let _ = handle.update(cx, |_, window, cx| match command {
                Command::Copy => {
                    let text = TextSelection::selected_text(window, cx);
                    let text = text.trim();
                    if !text.is_empty() {
                        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text.to_string()));
                        crate::clipboard::push(cx);
                    }
                    TextSelection::close_edit_menu(window, cx);
                }
                Command::SelectAll => TextSelection::select_all(window, cx),
                Command::Cut | Command::Paste => {}
            });
        }) as Run
    };
    follow(owner, step, window.scale_factor(), items, run);
}

pub(crate) fn init(cx: &mut App) {
    if cfg!(target_os = "android") {
        watch(cx);
    }
}

/// Follows every input and text selection from now on.
fn watch(cx: &mut App) {
    watch_inputs::<InputMode>(cx);
    watch_inputs::<TextareaMode>(cx);
    watch_inputs::<EditorMode>(cx);
    cx.observe_new(|_: &mut Root, window, cx| {
        let Some(window) = window else { return };
        let handle = window.window_handle();
        TextSelection::observe_touch_selection(window, cx, move |cx| {
            let _ = handle.update(cx, |_, window, cx| follow_text(window, cx));
        })
        .detach();
    })
    .detach();
    system::listen(cx);
}

/// `GpuiEditMenu.java`.
#[cfg(target_os = "android")]
mod system {
    use jni::objects::JValue;

    use gpui_kit::App;

    const CLASS: &str = "dev.gpui.glue.GpuiEditMenu";

    pub(super) fn show(at: [i32; 4], items: i32) {
        let [left, top, right, bottom] = at;
        let done = crate::android::with_class(CLASS, |env, activity, class| {
            env.call_static_method(
                class,
                jni::jni_str!("show"),
                jni::jni_sig!("(Landroid/app/Activity;IIIII)V"),
                &[
                    JValue::Object(activity),
                    JValue::Int(left),
                    JValue::Int(top),
                    JValue::Int(right),
                    JValue::Int(bottom),
                    JValue::Int(items),
                ],
            )
            .map(drop)
        });
        if let Err(error) = done {
            tracing::warn!(%error, "could not open the edit menu");
        }
    }

    pub(super) fn hide() {
        let done = crate::android::with_class(CLASS, |env, activity, class| {
            env.call_static_method(
                class,
                jni::jni_str!("hide"),
                jni::jni_sig!("(Landroid/app/Activity;)V"),
                &[JValue::Object(activity)],
            )
            .map(drop)
        });
        if let Err(error) = done {
            tracing::warn!(%error, "could not close the edit menu");
        }
    }

    /// Commands from `GpuiEditMenu.nativeCommand`, which runs on the system's
    /// UI thread, to the application's.
    fn commands() -> &'static (async_channel::Sender<i32>, async_channel::Receiver<i32>) {
        static COMMANDS: std::sync::OnceLock<(async_channel::Sender<i32>, async_channel::Receiver<i32>)> =
            std::sync::OnceLock::new();
        COMMANDS.get_or_init(async_channel::unbounded)
    }

    pub(super) fn listen(cx: &mut App) {
        let commands = commands().1.clone();
        cx.spawn(async move |cx| {
            while let Ok(code) = commands.recv().await {
                cx.update(|cx| super::heard(code, cx));
            }
        })
        .detach();
    }

    /// `GpuiEditMenu.nativeCommand`: a command chosen, or `0`, the toolbar
    /// closed by the system.
    #[unsafe(no_mangle)]
    pub extern "C" fn Java_dev_gpui_glue_GpuiEditMenu_nativeCommand(
        _env: *mut std::ffi::c_void,
        _class: *mut std::ffi::c_void,
        code: i32,
    ) {
        let _ = commands().0.try_send(code);
    }
}

/// No system menu off Android: the toolkit's stays (`init` does nothing).
#[cfg(not(target_os = "android"))]
mod system {
    pub(super) fn show(_: [i32; 4], _: i32) {}
    pub(super) fn hide() {}
    pub(super) fn listen(_: &mut gpui_kit::App) {}
}

#[cfg(test)]
mod tests {
    use gpui_kit::{Bounds, point, px, size};

    use super::{Command, Seen, Step, items, physical, step};

    fn at() -> Bounds<gpui_kit::Pixels> {
        Bounds::new(point(px(10.), px(20.)), size(px(30.), px(16.)))
    }

    fn seen(menu_open: bool) -> Option<Seen> {
        Some(Seen { menu_open, dragging: false, bounds: Some(at()) })
    }

    #[test]
    fn the_toolkits_menu_opening_opens_the_systems() {
        assert_eq!(step(seen(true), false), Step::Over(at()));
    }

    #[test]
    fn once_open_it_follows_the_selection() {
        // The toolkit's is closed by then.
        assert_eq!(step(seen(false), true), Step::Over(at()));
    }

    #[test]
    fn a_selection_with_no_menu_gets_none() {
        assert_eq!(step(seen(false), false), Step::Leave);
    }

    #[test]
    fn it_steps_aside_while_a_handle_is_dragged() {
        let dragged = Some(Seen { menu_open: false, dragging: true, bounds: Some(at()) });
        assert_eq!(step(dragged, true), Step::Away);
    }

    #[test]
    fn it_goes_with_the_selection() {
        assert_eq!(step(None, true), Step::Away);
        assert_eq!(step(None, false), Step::Leave);
    }

    #[test]
    fn a_selection_scrolled_away_takes_it_away() {
        let away = Some(Seen { menu_open: true, dragging: false, bounds: None });
        assert_eq!(step(away, true), Step::Away);
        assert_eq!(step(away, false), Step::Leave);
    }

    #[test]
    fn the_bits_are_the_java_sides() {
        // GpuiEditMenu: CUT 1, COPY 2, PASTE 4, SELECT_ALL 8.
        assert_eq!(items([Command::Cut, Command::Copy, Command::Paste, Command::SelectAll]), 0b1111);
        assert_eq!(items([Command::Paste]), 4);
        assert_eq!(Command::from_code(4), Some(Command::SelectAll));
        assert_eq!(Command::from_code(0), None);
    }

    #[test]
    fn bounds_go_to_physical_pixels() {
        assert_eq!(physical(at(), 2.5), [25, 50, 100, 90]);
    }
}

/// With a window: the toolkit's menu closed, and what would be offered.
#[cfg(test)]
mod in_a_window {
    use gpui::{
        AnyWindowHandle, AppContext as _, Bounds, Context, Entity, InputEvent as _, InteractiveElement as _, IntoElement, LongPressEvent, ParentElement as _, Pixels,
        Point, Render, Styled as _, TestAppContext, TouchPhase, Window, WindowBounds, WindowOptions, div, point, px,
    };
    use gpui_kit::base::Root;
    use gpui_kit::component::input::{Input, InputState};
    use gpui_kit::component::text::{TextView, TextViewState};
    use gpui_kit::test::{TestSupportExt as _, TestWindowExt as _};

    use super::{Command, OPEN, Owner, heard, items, watch};

    struct Screen {
        input: Entity<InputState>,
        text: Entity<TextViewState>,
    }

    impl Render for Screen {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .flex()
                .flex_col()
                .gap_4()
                .p_4()
                .child(Input::new(&self.input).id("input").w(px(320.)))
                .child(div().id("text").test_support().w(px(320.)).child(TextView::new(&self.text).selectable(true)))
        }
    }

    fn screen(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Screen>) {
        cx.update(|cx| {
            gpui_kit::component::init(cx);
            watch(cx);
            let bounds = Bounds::maximized(None, cx);
            let (window, screen) = gpui_kit::open_window(
                WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() },
                cx,
                |window, cx| {
                    cx.new(|cx| Screen {
                        input: cx.new(|cx| InputState::new(window, cx).default_value("quick select value")),
                        text: cx.new(|cx| TextViewState::markdown("quick select value", cx)),
                    })
                },
            )
            .expect("a test window");
            let _ = window.downcast::<Root>().expect("the toolkit's root");
            (window, screen)
        })
    }

    /// A long press on the element `id`, at the point `at` picks in its
    /// bounds, as the platform delivers one: each phase an event of its own,
    /// and a frame drawn once the event's effects are done.
    fn long_press(
        cx: &mut TestAppContext,
        window: AnyWindowHandle,
        id: &'static str,
        at: impl Fn(Bounds<Pixels>) -> Point<Pixels>,
    ) {
        let position = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                at(window.find(id).bounds())
            })
            .unwrap();
        for phase in [TouchPhase::Started, TouchPhase::Ended] {
            cx.update_window(window, |_, window, cx| {
                window.dispatch_event(LongPressEvent { phase, start_position: position, position }.to_platform_input(), cx);
            })
            .unwrap();
            cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
        }
    }

    /// Whether the window draws something called `label`: the toolkit's menu.
    fn draws(cx: &mut TestAppContext, window: AnyWindowHandle, label: &'static str) -> bool {
        cx.update_window(window, |_, window, _| window.try_find(label).is_some()).unwrap()
    }

    fn in_the_input(bounds: Bounds<Pixels>) -> Point<Pixels> {
        point(bounds.left() + px(24.), bounds.center().y)
    }

    fn open() -> Option<(Owner, i32)> {
        OPEN.with_borrow(|open| open.as_ref().map(|open| (open.owner, open.items)))
    }

    #[gpui::test]
    fn an_input_gets_the_systems_menu_and_not_the_toolkits(cx: &mut TestAppContext) {
        let (window, screen) = screen(cx);
        let input = cx.read(|cx| screen.read(cx).input.clone());
        long_press(cx, window, "input", in_the_input);
        assert!(!draws(cx, window, "Copy"), "the toolkit's menu is not drawn");
        let all = items([Command::Cut, Command::Copy, Command::Paste, Command::SelectAll]);
        assert_eq!(open(), Some((Owner::Input(input.entity_id()), all)));

        cx.update(|cx| heard(Command::Copy as i32, cx));
        let copied = cx.read(|cx| cx.read_from_clipboard().and_then(|item| item.text()));
        assert_eq!(copied.as_deref(), Some("quick"));

        // The toolbar closes itself after a copy, and says so.
        cx.update(|cx| heard(0, cx));
        assert_eq!(open(), None);
    }

    #[gpui::test]
    fn select_all_leaves_the_menu_with_less_to_offer(cx: &mut TestAppContext) {
        let (window, screen) = screen(cx);
        let input = cx.read(|cx| screen.read(cx).input.clone());
        long_press(cx, window, "input", in_the_input);
        cx.update(|cx| heard(Command::SelectAll as i32, cx));
        cx.update_window(window, |_, window, cx| window.render_frame(cx)).unwrap();
        assert_eq!(cx.read(|cx| input.read(cx).selected_range()), 0.."quick select value".len());
        assert!(!draws(cx, window, "Copy"));
        let left = items([Command::Cut, Command::Copy, Command::Paste]);
        assert_eq!(open(), Some((Owner::Input(input.entity_id()), left)));
    }

    #[gpui::test]
    fn text_gets_copy_and_select_all(cx: &mut TestAppContext) {
        let (window, _) = screen(cx);
        long_press(cx, window, "text", |bounds| point(bounds.left() + px(8.), bounds.top() + px(10.)));
        assert!(!draws(cx, window, "Copy"), "the toolkit's menu is not drawn");
        assert_eq!(open(), Some((Owner::Text(window), items([Command::Copy, Command::SelectAll]))));
    }
}
