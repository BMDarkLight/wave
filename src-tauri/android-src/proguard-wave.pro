# Keep JNI entry points visible to native code (R8 cannot see JNI callers).
-keep class app.bmdarklight.wave.FolderPickerCallback { *; }
-keepclassmembers class app.bmdarklight.wave.FolderPickerCallback {
    public static java.lang.String[] pickForJni(android.app.Activity);
    public static *** pick(android.app.Activity);
}
-keep class app.bmdarklight.wave.FolderPickerCallback$* { *; }
-keep class app.bmdarklight.wave.SafMediaScanner { *; }
-keepclassmembers class app.bmdarklight.wave.SafMediaScanner {
    public static java.lang.String[] listAudioFiles(android.app.Activity, java.lang.String);
}
-keep class app.bmdarklight.wave.MediaNativeBridge { *; }
-keepclassmembers class app.bmdarklight.wave.MediaNativeBridge {
    public static void dispatch(java.lang.String);
    private static native void nativeOnMediaAction(java.lang.String);
}
-keep class app.bmdarklight.wave.MediaMetadataProbe { *; }
-keepclassmembers class app.bmdarklight.wave.MediaMetadataProbe {
    public static java.lang.String probe(android.app.Activity, java.lang.String);
}
-keep class app.bmdarklight.wave.audio.WaveExoPlayer { *; }
-keepclassmembers class app.bmdarklight.wave.audio.WaveExoPlayer {
    public static app.bmdarklight.wave.audio.WaveExoPlayer getOrCreate(android.content.Context);
    public void playUri(java.lang.String);
    public void play();
    public void pause();
    public void stop();
    public void seekTo(long);
    public void setVolume(float);
    public long getCurrentPosition();
    public long getDuration();
    public boolean isPlaying();
    public boolean isEnded();
}
