package app.bmdarklight.wave;

import androidx.activity.ComponentActivity;
import androidx.activity.result.ActivityResultLauncher;
import androidx.activity.result.contract.ActivityResultContract;
import androidx.annotation.Keep;
import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import androidx.fragment.app.Fragment;
import androidx.fragment.app.FragmentActivity;

import android.content.Context;
import android.content.Intent;
import android.app.Activity;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.DocumentsContract;
import android.util.Log;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;

/**
 * SAF folder picker that is safe to invoke from JNI at any time.
 *
 * Late {@code registerForActivityResult} on a resumed Activity crashes; we add a
 * short-lived Fragment so registration happens in the Fragment's {@code onCreate}
 * (before that Fragment is STARTED). All failures complete the future instead of
 * throwing out of the Activity.
 *
 * JNI must call {@link #pickForJni(Activity)} — not {@link #pick(Activity)}.
 * The JNI-friendly entry returns {@code String[]} so method lookup does not
 * depend on {@code CompletableFuture} (which R8 / desugar can break for JNI).
 */
@Keep
public final class FolderPickerCallback {
    private static final String TAG = "FolderPickerCallback";
    private static final String FRAGMENT_TAG = "wave_folder_picker";
    private static final long PICK_TIMEOUT_SECONDS = 120L;

    private FolderPickerCallback() {}

    @Keep
    public static class FolderPickerResult {
        @Keep public final String uri;
        @Keep public final String displayName;

        public FolderPickerResult(String uri, String displayName) {
            this.uri = uri;
            this.displayName = displayName;
        }
    }

    /**
     * JNI entry point.
     *
     * @return {@code null} if the user cancelled; otherwise
     *         {@code { uri, displayName }} (displayName may be empty).
     * @throws Exception on failure (message is surfaced to the app UI)
     */
    @Keep
    @Nullable
    public static String[] pickForJni(@Nullable Activity activity) throws Exception {
        if (activity == null) {
            throw new IllegalStateException("Folder picker: Activity is null");
        }
        // Must not run on the main thread — we block on the future below.
        if (activity.getMainLooper().getThread() == Thread.currentThread()) {
            throw new IllegalStateException(
                "Folder picker: pickForJni must not be called on the UI thread");
        }

        try {
            FolderPickerResult result = pick(activity).get(PICK_TIMEOUT_SECONDS, TimeUnit.SECONDS);
            if (result == null) {
                return null; // cancelled
            }
            String name = result.displayName != null ? result.displayName : "";
            return new String[] { result.uri, name };
        } catch (TimeoutException e) {
            throw new RuntimeException("Folder picker timed out waiting for a folder", e);
        } catch (ExecutionException e) {
            Throwable cause = e.getCause() != null ? e.getCause() : e;
            String msg = safeMessage(cause);
            if (cause instanceof Exception) {
                throw (Exception) cause;
            }
            throw new RuntimeException("Folder picker failed: " + msg, cause);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RuntimeException("Folder picker interrupted", e);
        }
    }

    /**
     * Launch the tree picker and return a future.
     * Completes with {@code null} on cancel, or exceptionally with a message on failure.
     */
    @NonNull
    public static CompletableFuture<FolderPickerResult> pick(@Nullable Activity activity) {
        final CompletableFuture<FolderPickerResult> future = new CompletableFuture<>();

        if (activity == null) {
            future.completeExceptionally(new IllegalStateException(
                "Folder picker: Activity is null"));
            return future;
        }

        final Runnable launch = () -> {
            try {
                if (!(activity instanceof FragmentActivity)) {
                    future.completeExceptionally(new IllegalStateException(
                        "Folder picker: Activity is not a FragmentActivity ("
                            + activity.getClass().getName()
                            + "). Cannot register SAF launcher safely."));
                    return;
                }

                FragmentActivity fa = (FragmentActivity) activity;
                // Drop any leftover picker fragment from a previous attempt.
                Fragment existing = fa.getSupportFragmentManager().findFragmentByTag(FRAGMENT_TAG);
                if (existing != null) {
                    fa.getSupportFragmentManager()
                        .beginTransaction()
                        .remove(existing)
                        .commitNowAllowingStateLoss();
                }

                PickerFragment fragment = PickerFragment.newInstance();
                fragment.attachFuture(future);
                fa.getSupportFragmentManager()
                    .beginTransaction()
                    .add(fragment, FRAGMENT_TAG)
                    .commitNowAllowingStateLoss();
            } catch (Throwable t) {
                Log.e(TAG, "Failed to start folder picker", t);
                if (!future.isDone()) {
                    future.completeExceptionally(new RuntimeException(
                        "Folder picker failed to start: " + safeMessage(t), t));
                }
            }
        };

        try {
            if (activity.getMainLooper().getThread() == Thread.currentThread()) {
                launch.run();
            } else {
                activity.runOnUiThread(launch);
            }
        } catch (Throwable t) {
            Log.e(TAG, "Failed to post folder picker to UI thread", t);
            future.completeExceptionally(new RuntimeException(
                "Folder picker could not reach UI thread: " + safeMessage(t), t));
        }

        return future;
    }

    /** Keep JNI-friendly instance constructors so older call sites still resolve. */
    public FolderPickerCallback(Activity activity) {
        // no-op; use pickForJni(Activity)
    }

    public FolderPickerCallback(ComponentActivity activity) {
        // no-op; use pickForJni(Activity)
    }

    /** @deprecated Use {@link #pickForJni(Activity)} from JNI. */
    public CompletableFuture<FolderPickerResult> pickFolder() {
        CompletableFuture<FolderPickerResult> future = new CompletableFuture<>();
        future.completeExceptionally(new UnsupportedOperationException(
            "FolderPickerCallback.pickFolder() is deprecated; JNI must call pickForJni(Activity)"));
        return future;
    }

    static String safeMessage(Throwable t) {
        if (t == null) return "unknown error";
        String msg = t.getMessage();
        if (msg == null || msg.isEmpty()) {
            return t.getClass().getName();
        }
        return t.getClass().getSimpleName() + ": " + msg;
    }

    public static final class PickerFragment extends Fragment {
        private CompletableFuture<FolderPickerResult> future;
        private ActivityResultLauncher<Uri> launcher;
        private boolean launched;

        static PickerFragment newInstance() {
            return new PickerFragment();
        }

        void attachFuture(CompletableFuture<FolderPickerResult> future) {
            this.future = future;
        }

        @Override
        public void onCreate(@Nullable Bundle savedInstanceState) {
            super.onCreate(savedInstanceState);
            try {
                // Custom contract so the grant includes PERSISTABLE + PREFIX flags.
                // Stock OpenDocumentTree does not always request those, which makes
                // takePersistableUriPermission fail or only last for this session.
                launcher = registerForActivityResult(
                    new PersistableOpenDocumentTree(),
                    this::onTreePicked
                );
            } catch (Throwable t) {
                Log.e(TAG, "registerForActivityResult failed", t);
                completeExceptionally("registerForActivityResult failed: " + safeMessage(t), t);
                removeSelf();
            }
        }

        @Override
        public void onStart() {
            super.onStart();
            if (launched || launcher == null) return;
            launched = true;
            try {
                launcher.launch(null);
            } catch (Throwable t) {
                Log.e(TAG, "launcher.launch failed", t);
                completeExceptionally("Failed to open folder picker UI: " + safeMessage(t), t);
                removeSelf();
            }
        }

        private void onTreePicked(@Nullable Uri treeUri) {
            try {
                if (treeUri == null) {
                    // User cancelled — null result (not an exception).
                    if (future != null && !future.isDone()) {
                        future.complete(null);
                    }
                    return;
                }

                Activity activity = getActivity();
                if (activity == null) {
                    completeExceptionally("Folder picker: Activity gone after selection", null);
                    return;
                }

                persistUriPermission(activity, treeUri);
                String displayName = getDisplayName(activity, treeUri);
                if (future != null && !future.isDone()) {
                    future.complete(new FolderPickerResult(treeUri.toString(), displayName));
                }
            } catch (Throwable t) {
                Log.e(TAG, "onTreePicked failed", t);
                completeExceptionally("Folder picker result handling failed: " + safeMessage(t), t);
            } finally {
                removeSelf();
            }
        }

        private void completeExceptionally(String message, @Nullable Throwable cause) {
            if (future == null || future.isDone()) return;
            if (cause != null) {
                future.completeExceptionally(new RuntimeException(message, cause));
            } else {
                future.completeExceptionally(new RuntimeException(message));
            }
        }

        private void removeSelf() {
            try {
                if (isAdded()) {
                    getParentFragmentManager()
                        .beginTransaction()
                        .remove(this)
                        .commitAllowingStateLoss();
                }
            } catch (Throwable t) {
                Log.w(TAG, "Failed to remove picker fragment: " + safeMessage(t));
            }
        }

        private static void persistUriPermission(Activity activity, Uri uri) {
            // takePersistableUriPermission() ONLY accepts READ and/or WRITE flags.
            // Passing FLAG_GRANT_PERSISTABLE_URI_PERMISSION here throws
            // IllegalArgumentException (Preconditions.checkFlagsArgument).
            final int takeFlags = Intent.FLAG_GRANT_READ_URI_PERMISSION
                | Intent.FLAG_GRANT_WRITE_URI_PERMISSION;
            try {
                activity.getContentResolver().takePersistableUriPermission(uri, takeFlags);
                Log.d(TAG, "Persisted URI permission for: " + uri);
            } catch (SecurityException e) {
                // Non-fatal — scan may still work for this session.
                Log.w(TAG, "Failed to persist URI permission: " + e.getMessage());
            } catch (IllegalArgumentException e) {
                // OEM / flag mismatch — try read-only, then continue without persist.
                Log.w(TAG, "takePersistableUriPermission(R+W) rejected, retrying READ: "
                    + e.getMessage());
                try {
                    activity.getContentResolver().takePersistableUriPermission(
                        uri, Intent.FLAG_GRANT_READ_URI_PERMISSION);
                    Log.d(TAG, "Persisted READ URI permission for: " + uri);
                } catch (Exception retry) {
                    Log.w(TAG, "Could not persist URI permission (session-only access): "
                        + retry.getMessage());
                }
            } catch (Exception e) {
                Log.w(TAG, "Could not persist URI permission (session-only access): "
                    + e.getMessage());
            }
        }

        private static String getDisplayName(Activity activity, Uri uri) {
            try (android.database.Cursor cursor = activity.getContentResolver()
                    .query(uri, null, null, null, null)) {
                if (cursor != null && cursor.moveToFirst()) {
                    int nameIndex = cursor.getColumnIndex(DocumentsContract.Document.COLUMN_DISPLAY_NAME);
                    if (nameIndex >= 0) {
                        String name = cursor.getString(nameIndex);
                        if (name != null && !name.isEmpty()) return name;
                    }
                }
            } catch (Exception e) {
                Log.w(TAG, "Failed to get display name: " + e.getMessage());
            }
            return "Selected Folder";
        }
    }

    /**
     * Like {@link ActivityResultContracts.OpenDocumentTree}, but requests
     * persistable + prefix grants so {@code takePersistableUriPermission}
     * can keep access after the app restarts.
     */
    @Keep
    static final class PersistableOpenDocumentTree extends ActivityResultContract<Uri, Uri> {
        @NonNull
        @Override
        public Intent createIntent(@NonNull Context context, @Nullable Uri input) {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
            intent.addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION
                    | Intent.FLAG_GRANT_WRITE_URI_PERMISSION
                    | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION
                    | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION
            );
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && input != null) {
                intent.putExtra(DocumentsContract.EXTRA_INITIAL_URI, input);
            }
            return intent;
        }

        @Override
        public Uri parseResult(int resultCode, @Nullable Intent intent) {
            if (resultCode != Activity.RESULT_OK || intent == null) {
                return null;
            }
            return intent.getData();
        }
    }
}
