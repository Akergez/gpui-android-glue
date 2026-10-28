package dev.gpui.glue;

import android.app.Activity;
import android.os.Build;
import android.window.BackEvent;
import android.window.OnBackAnimationCallback;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

/**
 * The system's "back" for the Rust side (`back.rs` in gpui-android-glue).
 *
 * Two ways back reaches an application, and both end in the same place:
 *
 * - As the key KEYCODE_BACK, which NativeActivity hands to the native side
 *   before anything in Java sees it, and gpui-mobile turns into the keystroke
 *   "escape". That is how back arrives without predictive back: before
 *   Android 13.
 * - With predictive back, which the library's manifest turns on
 *   (`android:enableOnBackInvokedCallback`), the gesture is no key at all: the
 *   system calls an OnBackInvokedCallback, or with none registered animates
 *   out to the home screen itself. So one is registered exactly while the
 *   application has something to take back (Rust says so through
 *   setEnabled), and it hands the gesture to Rust as nativeBack; with nothing
 *   to take back the system's own animation runs.
 *
 * From Android 14 the callback is an OnBackAnimationCallback, and the
 * gesture's progress goes to Rust as it goes, for the page to move with it.
 */
public final class GpuiBack {
    /** An OnBackInvokedCallback; kept as an Object so that older systems never load the class. */
    private static Object callback;
    private static boolean registered;

    private GpuiBack() {}

    /** Whether the application has something to take back. Called by Rust, from any thread. */
    public static void setEnabled(Activity activity, boolean enabled) {
        if (Build.VERSION.SDK_INT < 33) return;
        activity.runOnUiThread(() -> apply(activity, enabled));
    }

    private static void apply(Activity activity, boolean enabled) {
        if (Build.VERSION.SDK_INT < 33 || enabled == registered) return;
        if (callback == null) {
            callback = Build.VERSION.SDK_INT >= 34 ? Animated.make() : (OnBackInvokedCallback) GpuiBack::nativeBack;
        }
        OnBackInvokedDispatcher dispatcher = activity.getOnBackInvokedDispatcher();
        if (enabled) {
            dispatcher.registerOnBackInvokedCallback(
                    OnBackInvokedDispatcher.PRIORITY_DEFAULT, (OnBackInvokedCallback) callback);
        } else {
            dispatcher.unregisterOnBackInvokedCallback((OnBackInvokedCallback) callback);
        }
        registered = enabled;
    }

    /** Android 14's callback, in a class of its own that older systems never load. */
    private static final class Animated {
        static OnBackInvokedCallback make() {
            return new OnBackAnimationCallback() {
                @Override
                public void onBackStarted(BackEvent event) {
                    nativeBackStarted(event.getSwipeEdge(), event.getProgress());
                }

                @Override
                public void onBackProgressed(BackEvent event) {
                    nativeBackProgressed(event.getProgress());
                }

                @Override
                public void onBackCancelled() {
                    nativeBackCancelled();
                }

                @Override
                public void onBackInvoked() {
                    nativeBack();
                }
            };
        }
    }

    /**
     * What back does when the application had nothing to take back and the
     * key came to it anyway: what the system does for a launcher activity,
     * leave it running under the home screen.
     */
    public static void moveToBack(Activity activity) {
        activity.runOnUiThread(() -> activity.moveTaskToBack(true));
    }

    /** The activity is gone, and its dispatcher with it. */
    public static void forget() {
        registered = false;
    }

    private static native void nativeBack();

    private static native void nativeBackStarted(int edge, float progress);

    private static native void nativeBackProgressed(float progress);

    private static native void nativeBackCancelled();
}
