package app.bmdarklight.wave.audio;

import android.content.Context;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import androidx.media3.common.AudioAttributes;
import androidx.media3.common.C;
import androidx.media3.common.MediaItem;
import androidx.media3.common.PlaybackException;
import androidx.media3.common.Player;
import androidx.media3.exoplayer.ExoPlayer;

/**
 * Lightweight ExoPlayer holder for Wave.
 *
 * Owns decode/output only. MediaSession notifications stay with
 * tauri-plugin-media-session + MediaNativeBridge.
 *
 * All public methods are safe to call from any thread; work is posted to
 * the main looper when required by ExoPlayer.
 */
public final class WaveExoPlayer {
    private static final String TAG = "WaveExoPlayer";

    private static volatile WaveExoPlayer INSTANCE;

    private final Context appContext;
    private final Handler mainHandler;
    private ExoPlayer player;
    private volatile boolean ended;

    private WaveExoPlayer(Context context) {
        this.appContext = context.getApplicationContext();
        this.mainHandler = new Handler(Looper.getMainLooper());
        runOnMainBlocking(this::initPlayer);
    }

    public static WaveExoPlayer getOrCreate(Context context) {
        WaveExoPlayer existing = INSTANCE;
        if (existing != null) {
            return existing;
        }
        synchronized (WaveExoPlayer.class) {
            if (INSTANCE == null) {
                INSTANCE = new WaveExoPlayer(context);
            }
            return INSTANCE;
        }
    }

    private void initPlayer() {
        if (player != null) {
            return;
        }
        AudioAttributes attrs = new AudioAttributes.Builder()
                .setUsage(C.USAGE_MEDIA)
                .setContentType(C.AUDIO_CONTENT_TYPE_MUSIC)
                .build();

        player = new ExoPlayer.Builder(appContext)
                .setAudioAttributes(attrs, /* handleAudioFocus= */ true)
                .setHandleAudioBecomingNoisy(true)
                .build();

        player.addListener(new Player.Listener() {
            @Override
            public void onPlaybackStateChanged(int playbackState) {
                ended = playbackState == Player.STATE_ENDED;
            }

            @Override
            public void onPlayerError(PlaybackException error) {
                Log.e(TAG, "Playback error: " + error.getMessage());
                ended = true;
            }
        });
    }

    public void playUri(String uriString) {
        if (uriString == null || uriString.isEmpty()) {
            throw new IllegalArgumentException("uri is empty");
        }
        runOnMainBlocking(() -> {
            initPlayer();
            ended = false;
            Uri uri = Uri.parse(normalizeUri(uriString));
            player.setMediaItem(MediaItem.fromUri(uri));
            player.prepare();
            player.play();
        });
    }

    public void play() {
        runOnMainBlocking(() -> {
            if (player != null) {
                ended = false;
                player.play();
            }
        });
    }

    public void pause() {
        runOnMainBlocking(() -> {
            if (player != null) {
                player.pause();
            }
        });
    }

    public void stop() {
        runOnMainBlocking(() -> {
            if (player != null) {
                player.stop();
                player.clearMediaItems();
                ended = false;
            }
        });
    }

    public void seekTo(long positionMs) {
        runOnMainBlocking(() -> {
            if (player != null) {
                ended = false;
                player.seekTo(Math.max(0L, positionMs));
            }
        });
    }

    public void setVolume(float volume) {
        float clamped = Math.max(0f, Math.min(1f, volume));
        runOnMainBlocking(() -> {
            if (player != null) {
                player.setVolume(clamped);
            }
        });
    }

    public long getCurrentPosition() {
        return queryOnMain(() -> player != null ? player.getCurrentPosition() : 0L);
    }

    public long getDuration() {
        return queryOnMain(() -> {
            if (player == null) {
                return 0L;
            }
            long d = player.getDuration();
            return d == C.TIME_UNSET ? 0L : d;
        });
    }

    public boolean isPlaying() {
        return queryOnMain(() -> player != null && player.isPlaying());
    }

    public boolean isEnded() {
        return ended || queryOnMain(() ->
                player != null && player.getPlaybackState() == Player.STATE_ENDED);
    }

    private static String normalizeUri(String uriString) {
        if (uriString.startsWith("content://")
                || uriString.startsWith("file://")
                || uriString.startsWith("http://")
                || uriString.startsWith("https://")) {
            return uriString;
        }
        // Absolute filesystem path → file:// URI
        if (uriString.startsWith("/")) {
            return Uri.fromFile(new java.io.File(uriString)).toString();
        }
        return uriString;
    }

    private void runOnMainBlocking(Runnable action) {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            action.run();
            return;
        }
        final Object lock = new Object();
        final Throwable[] error = new Throwable[1];
        synchronized (lock) {
            mainHandler.post(() -> {
                try {
                    action.run();
                } catch (Throwable t) {
                    error[0] = t;
                } finally {
                    synchronized (lock) {
                        lock.notifyAll();
                    }
                }
            });
            try {
                lock.wait(15_000);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                throw new RuntimeException("Interrupted waiting for ExoPlayer main-thread work", e);
            }
        }
        if (error[0] != null) {
            if (error[0] instanceof RuntimeException) {
                throw (RuntimeException) error[0];
            }
            throw new RuntimeException(error[0]);
        }
    }

    private interface LongQuery {
        long get();
    }

    private interface BoolQuery {
        boolean get();
    }

    private long queryOnMain(LongQuery query) {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return query.get();
        }
        final long[] result = new long[1];
        final Object lock = new Object();
        synchronized (lock) {
            mainHandler.post(() -> {
                result[0] = query.get();
                synchronized (lock) {
                    lock.notifyAll();
                }
            });
            try {
                lock.wait(5_000);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
        }
        return result[0];
    }

    private boolean queryOnMain(BoolQuery query) {
        if (Looper.myLooper() == Looper.getMainLooper()) {
            return query.get();
        }
        final boolean[] result = new boolean[1];
        final Object lock = new Object();
        synchronized (lock) {
            mainHandler.post(() -> {
                result[0] = query.get();
                synchronized (lock) {
                    lock.notifyAll();
                }
            });
            try {
                lock.wait(5_000);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
        }
        return result[0];
    }
}
