package net.dmsg.android;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.FileNotFoundException;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

/** Installed only in the test APK, under a different UID from the app. */
public class IncomingFixtureProvider extends ContentProvider {
    private static final byte[] SAMPLE = "External sample share, never a real secret.".getBytes(StandardCharsets.UTF_8);

    @Override public boolean onCreate() { return true; }

    @Override public String getType(Uri uri) { return "/large-probe".equals(uri.getPath()) ? "application/octet-stream" : "text/plain"; }

    @Override public Cursor query(Uri uri, String[] projection, String selection, String[] args, String sort) {
        if ("/autofill-state".equals(uri.getPath()) || "/autofill-finish".equals(uri.getPath())) {
            AutofillFixtureActivity activity = AutofillFixtureActivity.current.get();
            MatrixCursor result = new MatrixCursor(new String[] { "password", "uid" });
            result.addRow(new Object[] { activity == null ? "missing" : activity.password.getText().toString(), android.os.Process.myUid() });
            if (activity != null && "/autofill-finish".equals(uri.getPath())) activity.runOnUiThread(activity::finish);
            return result;
        }
        if ("/probe-documents".equals(uri.getPath())) {
            boolean denied = false;
            try {
                Uri target = android.provider.DocumentsContract.buildChildDocumentsUri(uri.getQueryParameter("authority"), "root");
                Cursor result = getContext().getContentResolver().query(target, null, null, null, null);
                if (result != null) result.close();
            } catch (SecurityException expected) { denied = true; }
            MatrixCursor result = new MatrixCursor(new String[] { "denied", "uid" });
            result.addRow(new Object[] { denied ? 1 : 0, android.os.Process.myUid() });
            return result;
        }
        String[] columns = projection != null ? projection : new String[] { OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE };
        MatrixCursor cursor = new MatrixCursor(columns);
        Object[] row = new Object[columns.length];
        for (int i = 0; i < columns.length; i++) {
            if (OpenableColumns.DISPLAY_NAME.equals(columns[i])) row[i] = "/large-probe".equals(uri.getPath()) ? "1000 MiB generated sample.bin" : "External sample.txt";
            else if (OpenableColumns.SIZE.equals(columns[i])) row[i] = "/large-probe".equals(uri.getPath()) ? 1000L * 1024 * 1024 : uri.getPath().equals("/oversized") ? 101L * 1024 * 1024 : SAMPLE.length;
        }
        cursor.addRow(row);
        return cursor;
    }

    @Override public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        boolean large = "/large-probe".equals(uri.getPath());
        if (!"r".equals(mode) || (!"/sample".equals(uri.getPath()) && !large)) throw new FileNotFoundException();
        try {
            ParcelFileDescriptor[] pipe = ParcelFileDescriptor.createPipe();
            new Thread(() -> {
                try (ParcelFileDescriptor.AutoCloseOutputStream output = new ParcelFileDescriptor.AutoCloseOutputStream(pipe[1])) {
                    if (large) {
                        byte[] block = new byte[64 * 1024];
                        for (int i = 0; i < 16000; i++) output.write(block);
                    } else output.write(SAMPLE);
                }
                catch (IOException ignored) { }
            }, "dmsg-test-share").start();
            return pipe[0];
        } catch (IOException error) { throw new FileNotFoundException(error.getMessage()); }
    }

    @Override public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException(); }
    @Override public int delete(Uri uri, String selection, String[] args) { throw new UnsupportedOperationException(); }
    @Override public int update(Uri uri, ContentValues values, String selection, String[] args) { throw new UnsupportedOperationException(); }
}
