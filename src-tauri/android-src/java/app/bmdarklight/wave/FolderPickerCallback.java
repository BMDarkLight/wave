package app.bmdarklight.wave;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.provider.DocumentsContract;
import android.util.Log;

import androidx.activity.ComponentActivity;
import androidx.activity.result.ActivityResultLauncher;
import androidx.activity.result.contract.ActivityResultContracts;
import androidx.annotation.NonNull;
import androidx.annotation.Nullable;
import androidx.fragment.app.Fragment;
import androidx.fragment.app.FragmentActivity;

import java.util.concurrent.CompletableFuture;

/**
 * SAF folder picker that is safe to invoke from JNI at any time.
 *
 * Late {@code registerForActivityResult} on a resumed Activity crashes; we add a
 * short-lived Fragment so registration happens in the Fragment's {@code onCreate}
 * (before that Fragment is STARTED). All failures complete the future instead of
 * throwing out of the Activity.
 */
public final class FolderPickerCallback {
    private static final String TAG = "FolderPickerCallback";
    private static final String FRAGMENT_TAG = "wave_folder_picker";

    private FolderPickerCallback() {}

    public static class FolderPickerResult {
        public final String uri;
        public final String displayName;

        public FolderPickerResult(String uri, String displayName) {
            this.uri = uri;
            this.displayName = displayName;
        }
    }

    /**
     * JNI entry: launch the tree picker and return a future.
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
        // no-op; use pick(Activity)
    }

    public FolderPickerCallback(ComponentActivity activity) {
        // no-op; use pick(Activity)
    }

    /** @deprecated Use {@link #pick(Activity)} */
    public CompletableFuture<FolderPickerResult> pickFolder() {
        CompletableFuture<FolderPickerResult> future = new CompletableFuture<>();
        future.completeExceptionally(new UnsupportedOperationException(
            "FolderPickerCallback.pickFolder() is deprecated; JNI must call FolderPickerCallback.pick(Activity)"));
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
                launcher = registerForActivityResult(
                    new ActivityResultContracts.OpenDocumentTree(),
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
            try {
                final int takeFlags = Intent.FLAG_GRANT_READ_URI_PERMISSION
                    | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION;
                activity.getContentResolver().takePersistableUriPermission(uri, takeFlags);
                Log.d(TAG, "Persisted URI permission for: " + uri);
            } catch (SecurityException e) {
                // Non-fatal — scan may still work for this session.
                Log.w(TAG, "Failed to persist URI permission: " + e.getMessage());
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
}
