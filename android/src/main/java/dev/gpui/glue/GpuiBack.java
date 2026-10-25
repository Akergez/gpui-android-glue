package dev.gpui.glue;

import android.app.Activity;
import android.os.Build;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

/**
 * The system's "back" for the Rust side (`back.rs` in gpui-android-glue).
 *
 * Two ways back reaches an application, and both end in the same place:
 *
 * - As the key KEYCODE_BACK, which NativeActivity hands to the native side
 *   before anything in Java sees it, and gpui-mobile turns into the keystroke
 *   "escape". That is how back arrives without predictive back: the
 *   three-button bar, and the gesture of an application that has not opted
 *   in (`android:enableOnBackInvokedCallback`, or targetSdk 36 on Android 16).
 * - With predictive back, the gesture is no key at all: the system calls an
 *   OnBackInvokedCallback, or with none registered animates out to the home
 *   screen itself. So one is registered exactly while the application has
 *   something to take back (Rust says so through setEnabled), and it hands the
 *   gesture to Rust as nativeBack; with nothing to take back the system's own
 *   animation runs.
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
        if (callback == null) callback = (OnBackInvokedCallback) GpuiBack::nativeBack;
        OnBackInvokedDispatcher dispatcher = activity.getOnBackInvokedDispatcher();
        if (enabled) {
            dispatcher.registerOnBackInvokedCallback(
                    OnBackInvokedDispatcher.PRIORITY_DEFAULT, (OnBackInvokedCallback) callback);
        } else {
            dispatcher.unregisterOnBackInvokedCallback((OnBackInvokedCallback) callback);
        }
        registered = enabled;
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
}
