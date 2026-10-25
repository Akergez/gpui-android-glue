# Native code finds these classes, their methods and their native methods by
# name (gpui-mobile and gpui-android-glue's Rust half): nothing in them may be
# renamed or removed.
-keep class dev.gpui.mobile.** { *; }
-keep class dev.gpui.glue.** { *; }
-keep class android.app.NativeActivity { *; }
