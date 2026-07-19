package app.bmdarklight.wave;

import android.util.Log;

/**
 * JNI bridge so media notification / audio-focus / headset events can control
 * the Rust audio engine without going through the WebView.
 *
 * When the app is backgrounded, Android may freeze the WebView — JS media
 * handlers then never run. Native dispatch keeps play/pause/next working.
 */
public final class MediaNativeBridge {
    private static final String TAG = "MediaNativeBridge";

    private MediaNativeBridge() {}

    /** Called from the media-session plugin (via reflection). */
    public static void dispatch(String action) {
        if (action == null || action.isEmpty()) return;
        try {
            nativeOnMediaAction(action);
        } catch (UnsatisfiedLinkError e) {
            Log.w(TAG, "nativeOnMediaAction not registered yet: " + e.getMessage());
        } catch (Throwable t) {
            Log.e(TAG, "dispatch failed for action=" + action, t);
        }
    }

    private static native void nativeOnMediaAction(String action);
}
