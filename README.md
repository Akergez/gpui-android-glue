# gpui-android-glue

What a [gpui-kit](https://gpui-kit.com) application needs on Android beyond
[gpui-mobile](https://github.com/longbridge/gpui-mobile), in one place
instead of a copy in every application (matterfast, money-manager-ng,
aiddiction). Two halves, released together under one version:

- **the crate** `gpui-android-glue` (`src/`): `android_main`, the keyboard,
  back, copy and paste, insets, appearance, links, documents. Every function exists on every
  platform and does the desktop's thing, or nothing, off Android;
- **the Android library** `com.akergez.gpui:android-glue` (`android/`): the
  Java classes gpui-mobile calls by name (from its example, with
  matterfast's IME), what the crate calls, and gpui-mobile's helper
  activities in a manifest merged into the application's.

The halves call each other by name (`gpuiKeyboardHint`, `GpuiBack`,
`GpuiEditMenu`, `GpuiPathPicker.saveDocument`), so an application takes both at the same
version. `docs/PLAN.md` (in Russian) is why it is the way it is.

## In an application

`Cargo.toml`, at the gpui-mobile revision the crate names:

```toml
[dependencies]
gpui-android-glue = { git = "https://github.com/Akergez/gpui-android-glue", tag = "v0.4.0" }
```

`platform-verifier` hands the activity to `rustls-platform-verifier` at
start, for an application that connects over TLS with rustls.

`lib.rs`:

```rust
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: gpui_android_glue::launch::AndroidApp) {
    use gpui_android_glue::launch::{Config, run};
    run(app, Config::new("my-app").filter("warn,my_app=info"), gpui_kit::assets::Assets, start);
}
```

`run` calls `gpui_android_glue::init`; on a desktop call it yourself after
`gpui_kit::init`.

`android/settings.gradle.kts`:

```kotlin
dependencyResolutionManagement {
    repositories {
        google()
        mavenCentral()
        maven {
            url = uri("https://maven.pkg.github.com/akergez/gpui-android-glue")
            credentials {
                username = providers.environmentVariable("GITHUB_ACTOR").orNull
                    ?: providers.gradleProperty("gpr.user").orNull
                password = providers.environmentVariable("GITHUB_TOKEN").orNull
                    ?: providers.gradleProperty("gpr.key").orNull
            }
        }
    }
}
// A checkout of this repository beside the application, instead of the
// published library: `./gradlew -PglueLocal assembleDebug`.
if (providers.gradleProperty("glueLocal").isPresent) {
    includeBuild("../../gpui-android-glue/android")
}
```

`android/app/build.gradle.kts`: `implementation("com.akergez.gpui:android-glue:0.4.0")`,
in place of the androidx libraries the copied classes needed, and with the
copies of `dev/gpui/mobile/*.java` deleted from the application.

GitHub Packages asks for a token even to read a public package: at a desk, a
personal token with `read:packages` as `gpr.user`/`gpr.key` in
`~/.gradle/gradle.properties`; in an application's CI, `GITHUB_TOKEN`, once
the package's "Manage Actions access" lets that repository in.

### Back

Whatever back should close says so while it is open, and keeps the guard:

```rust
self.back = Some(gpui_android_glue::back::push(move |window, cx| {
    // Take back what this opened; `false` if there was nothing after all.
    true
}));
```

Back goes to the topmost layer after the toolkit's own uses of Escape
(dialogs, menus, inputs); with no layer to take it, the application goes
under the home screen. The library's manifest turns predictive back on
(`android:enableOnBackInvokedCallback`), so with no layer the system's own
animation takes the application home. See `src/back.rs`.

With predictive back the gesture over a toolkit dialog reaches the
application only if something is a layer: open dialogs with
`gpui_android_glue::back::open_dialog(window, cx, build)` in place of
`window.open_dialog`, and each is a layer for as long as it is open.

From Android 14 the gesture shows where it goes before it is done: the page
moves aside with the finger over the page back goes to, and goes back where
it was if the finger turns round. The guard says how far a gesture that
would go to its layer has gone; lay the page out by it with `preview`:

```rust
use gpui_android_glue::back;

let gesture = self.back.as_ref().and_then(|back| back.gesture(window, cx));
// Build what back goes to only while there is a gesture.
let under = gesture.map(|_| self.render_page(previous, window, cx));
back::preview(page, gesture)   // the whole page, opaque, filling the window
    .under(under)
    .swipe_back(true)          // and a finger drawn to the right, anywhere
```

`swipe_back` adds back by a finger drawn to the right across the page, with
the same preview, the page under the finger pixel for pixel; the system's gesture works as before. A page that scrolls
sideways would lose that direction to it: keep it to pages without.

### Copy and paste

Nothing to do: `init` makes it so. A long press in an input or in text
brings up the system's floating toolbar — Cut, Copy, Paste, Select all, in
the phone's language — in place of the toolkit's own menu, and its commands
are the input's own. The toolkit's clipboard, which gpui-mobile keeps inside
the process, is kept in step with the phone's: brought in before a paste,
handed out after a copy and whenever the window loses its focus. An
application that copies or pastes some other way and wants it seen at once
calls `gpui_android_glue::clipboard::{push, pull}` around it. See
`src/edit_menu.rs` and `src/clipboard.rs`.

### The keyboard and the bars

The window lies under the status bar, the navigation bar and the keyboard
edge to edge, and is told how much of it each one covers, frame by frame
while the keyboard slides in or out (`GpuiInsets.java`). Read them in
`render` and lay out by them; the window is drawn again on every change, so
what makes room for the keyboard moves with it, with nothing to animate:

```rust
use gpui_android_glue::{insets, keyboard};

let (top, bars) = insets::system_bars();       // the bars only, never the keyboard
let under = keyboard::keyboard_inset(window);  // the keyboard as drawn now, 0 when down
div().pt(top).pb(bars.max(under))              // room for whichever covers more
```

`keyboard::is_up` says where the keyboard is going, not where it is. The
activity keeps `android:windowSoftInputMode="adjustResize"`, which the
animation needs on Android 10 and older. On a desktop `GPUI_KEYBOARD=<height>`
pretends a keyboard is up.

## Checking it

```sh
podman build -t localhost/gpui-android-glue-build -f build-aux/build.Containerfile build-aux
podman run --rm --userns=keep-id --security-opt label=disable \
  -e HOME=/tmp -v "$PWD":/src -w /src localhost/gpui-android-glue-build build-aux/check.sh
```

`build-aux/ime-tests.sh` alone needs only a JDK.

## Releasing

Set `version` in `Cargo.toml`, `cargo update -p gpui-android-glue`, commit,
tag `vX.Y.Z`, push the tag. `.github/workflows/release.yml` checks that the
two agree and publishes the Android library under that version; the tag is
the crate's release.

## Licence

GPL-3.0. The classes in `android/src/main/java/dev/gpui/mobile` come from
gpui-mobile's example, under its terms (GPL-3.0-or-later, AGPL-3.0-or-later
or Apache-2.0).
