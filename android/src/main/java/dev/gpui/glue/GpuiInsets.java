package dev.gpui.glue;

import android.app.Activity;
import android.view.View;

import androidx.core.view.ViewCompat;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsAnimationCompat;
import androidx.core.view.WindowInsetsCompat;

import java.util.List;

/**
 * What the system's bars and the on-screen keyboard cover, for the Rust side
 * (`insets.rs` in gpui-android-glue), frame by frame while the keyboard moves.
 *
 * NativeActivity tells the native side of its content rect only, once the
 * layout is done: the keyboard arrived there in one step, after its own
 * animation had finished. Here the window is laid out edge to edge, so the
 * system resizes nothing and only reports, and the keyboard's animation is
 * followed: every frame of it hands the keyboard's height to Rust as it is
 * on screen. On Android 10 and older androidx animates the same way itself;
 * that needs `adjustResize`, which the applications already ask for.
 *
 * Sizes go to Rust in the window's physical pixels, as the system gives them.
 */
public final class GpuiInsets {
    /** The status bar's height, the navigation bar's, the keyboard's as drawn. */
    private static int top, bars, ime;
    /** Whether the keyboard is up, or on its way up. */
    private static boolean visible;
    /** Whether the keyboard is moving: its height comes from the animation then. */
    private static boolean moving;

    private GpuiInsets() {}

    /** Called once the activity has its content view, on the UI thread. */
    public static void install(Activity activity) {
        WindowCompat.setDecorFitsSystemWindows(activity.getWindow(), false);
        View content = activity.findViewById(android.R.id.content);
        ViewCompat.setOnApplyWindowInsetsListener(content, (view, insets) -> {
            top = insets.getInsets(WindowInsetsCompat.Type.systemBars()).top;
            bars = insets.getInsets(WindowInsetsCompat.Type.systemBars()).bottom;
            visible = insets.isVisible(WindowInsetsCompat.Type.ime());
            // At the start of an animation the insets are already where it
            // ends; the keyboard on screen is still where it starts.
            if (!moving) ime = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom;
            tell();
            return insets;
        });
        ViewCompat.setWindowInsetsAnimationCallback(content,
                new WindowInsetsAnimationCompat.Callback(
                        WindowInsetsAnimationCompat.Callback.DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
            @Override public void onPrepare(WindowInsetsAnimationCompat animation) {
                if (isIme(animation)) moving = true;
            }

            @Override public WindowInsetsCompat onProgress(WindowInsetsCompat insets,
                    List<WindowInsetsAnimationCompat> running) {
                if (moving) {
                    ime = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom;
                    tell();
                }
                return insets;
            }

            @Override public void onEnd(WindowInsetsAnimationCompat animation) {
                if (!isIme(animation)) return;
                moving = false;
                WindowInsetsCompat insets = ViewCompat.getRootWindowInsets(content);
                if (insets != null) ime = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom;
                tell();
            }
        });
        ViewCompat.requestApplyInsets(content);
    }

    private static boolean isIme(WindowInsetsAnimationCompat animation) {
        return (animation.getTypeMask() & WindowInsetsCompat.Type.ime()) != 0;
    }

    private static void tell() {
        nativeInsets(top, bars, ime, visible);
    }

    private static native void nativeInsets(int top, int bars, int ime, boolean visible);
}
