# Keep JNI entry points visible to native code (R8 cannot see JNI callers).
-keep class app.bmdarklight.wave.FolderPickerCallback { *; }
-keepclassmembers class app.bmdarklight.wave.FolderPickerCallback {
    public static java.lang.String[] pickForJni(android.app.Activity);
    public static *** pick(android.app.Activity);
}
-keep class app.bmdarklight.wave.FolderPickerCallback$* { *; }
-keep class app.bmdarklight.wave.MediaNativeBridge { *; }
-keepclassmembers class app.bmdarklight.wave.MediaNativeBridge {
    public static void dispatch(java.lang.String);
    private static native void nativeOnMediaAction(java.lang.String);
}
