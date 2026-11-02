package dev.gpui.glue;

import android.app.Activity;
import android.os.Build;
import android.view.Display;
import android.view.WindowManager;

/**
 * Asks for the display's fastest mode at its current resolution, through the
 * window's attributes. A frame-rate vote on the surface
 * ({@code ANativeWindow_setFrameRate}, which gpui-mobile casts) is honoured
 * by some vendors only: OnePlus keeps such an application at 60 Hz, while a
 * window's preferred display mode moves it.
 */
public final class GpuiRefreshRate {
    private GpuiRefreshRate() {}

    public static void install(Activity activity) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return;
        Display display = activity.getWindowManager().getDefaultDisplay();
        Display.Mode current = display.getMode();
        Display.Mode best = current;
        for (Display.Mode mode : display.getSupportedModes()) {
            boolean sameSize = mode.getPhysicalWidth() == current.getPhysicalWidth()
                    && mode.getPhysicalHeight() == current.getPhysicalHeight();
            if (sameSize && mode.getRefreshRate() > best.getRefreshRate()) best = mode;
        }
        WindowManager.LayoutParams params = activity.getWindow().getAttributes();
        params.preferredDisplayModeId = best.getModeId();
        params.preferredRefreshRate = best.getRefreshRate();
        activity.getWindow().setAttributes(params);
    }
}
