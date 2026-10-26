package dev.gpui.glue;

import android.app.Activity;
import android.content.ClipboardManager;
import android.content.Context;
import android.graphics.Rect;
import android.view.ActionMode;
import android.view.Menu;
import android.view.MenuItem;
import android.view.View;

/**
 * The system's edit menu over a selection made by touch, for the Rust side
 * (`edit_menu.rs` in gpui-android-glue): the floating toolbar a TextView
 * shows, with the system's own Cut, Copy, Paste and Select all.
 *
 * Rust says where the selection is and what to offer (show, hide); a command
 * chosen goes back to it as nativeCommand, and so does the toolbar closing by
 * itself (0), so that it is opened again when the selection asks for it.
 *
 * The toolbar is anchored to the activity's content view, which lies under
 * the whole of the native window: what Rust gives in the window's pixels is
 * moved into the view's.
 */
public final class GpuiEditMenu {
    // What show offers, one bit per command; nativeCommand's numbers are the
    // bit's place, counted from 1.
    private static final int CUT = 1, COPY = 2, PASTE = 4, SELECT_ALL = 8;

    /** The toolbar open now, or null. */
    private static ActionMode open;
    /** Over what, in the window's pixels. */
    private static final Rect where = new Rect();
    private static int items;

    private GpuiEditMenu() {}

    /** Opens the toolbar over the window's (left, top, right, bottom), or moves it there. */
    public static void show(Activity activity, int left, int top, int right, int bottom,
            int nextItems) {
        activity.runOnUiThread(() -> {
            where.set(left, top, right, bottom);
            if (open != null) {
                if (items != nextItems) {
                    items = nextItems;
                    open.invalidate();
                }
                open.invalidateContentRect();
                return;
            }
            items = nextItems;
            View content = activity.findViewById(android.R.id.content);
            if (content == null) return;
            open = content.startActionMode(new Callback(activity), ActionMode.TYPE_FLOATING);
        });
    }

    /** Closes the toolbar, if it is open. Not reported back: Rust asked. */
    public static void hide(Activity activity) {
        activity.runOnUiThread(() -> {
            ActionMode closing = open;
            open = null;
            if (closing != null) closing.finish();
        });
    }

    private static final class Callback extends ActionMode.Callback2 {
        private final Activity activity;

        Callback(Activity activity) {
            this.activity = activity;
        }

        @Override public boolean onCreateActionMode(ActionMode mode, Menu menu) {
            fill(menu);
            return true;
        }

        @Override public boolean onPrepareActionMode(ActionMode mode, Menu menu) {
            menu.clear();
            fill(menu);
            return true;
        }

        /** In TextView's order, with its titles: the phone's language. */
        private void fill(Menu menu) {
            if ((items & CUT) != 0) add(menu, android.R.id.cut, 1, android.R.string.cut);
            if ((items & COPY) != 0) add(menu, android.R.id.copy, 2, android.R.string.copy);
            if ((items & PASTE) != 0 && canPaste()) {
                add(menu, android.R.id.paste, 3, android.R.string.paste);
            }
            if ((items & SELECT_ALL) != 0) {
                add(menu, android.R.id.selectAll, 4, android.R.string.selectAll);
            }
        }

        private static void add(Menu menu, int id, int order, int title) {
            menu.add(Menu.NONE, id, order, title)
                    .setShowAsAction(MenuItem.SHOW_AS_ACTION_ALWAYS | MenuItem.SHOW_AS_ACTION_WITH_TEXT);
        }

        /** Whether there is anything to paste. Asking does not read it, so the system says nothing. */
        private boolean canPaste() {
            ClipboardManager clipboard =
                    (ClipboardManager) activity.getSystemService(Context.CLIPBOARD_SERVICE);
            return clipboard != null && clipboard.hasPrimaryClip();
        }

        @Override public boolean onActionItemClicked(ActionMode mode, MenuItem item) {
            int command;
            int id = item.getItemId();
            if (id == android.R.id.cut) command = 1;
            else if (id == android.R.id.copy) command = 2;
            else if (id == android.R.id.paste) command = 3;
            else if (id == android.R.id.selectAll) command = 4;
            else return false;
            nativeCommand(command);
            // As in a TextView: the toolbar has done its work, except for
            // Select all, which leaves it over the whole text.
            if (command != 4) mode.finish();
            return true;
        }

        @Override public void onDestroyActionMode(ActionMode mode) {
            // Closed by the system, or after a command: Rust is to know that
            // it is no longer open. Closed by hide, it knows.
            if (mode != open) return;
            open = null;
            nativeCommand(0);
        }

        @Override public void onGetContentRect(ActionMode mode, View view, Rect out) {
            int[] origin = new int[2];
            view.getLocationInWindow(origin);
            out.set(where);
            out.offset(-origin[0], -origin[1]);
        }
    }

    private static native void nativeCommand(int command);
}
