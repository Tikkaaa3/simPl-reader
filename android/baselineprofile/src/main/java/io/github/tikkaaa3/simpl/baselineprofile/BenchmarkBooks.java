package io.github.tikkaaa3.simpl.baselineprofile;

import android.content.ContentProvider;
import android.content.ContentValues;
import android.database.Cursor;
import android.database.MatrixCursor;
import android.net.Uri;
import android.os.ParcelFileDescriptor;
import android.provider.OpenableColumns;
import java.io.File;
import java.io.FileNotFoundException;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;

/** Only in the profiling APK; the production app has no benchmark entry point. */
public class BenchmarkBooks extends ContentProvider {
    @Override public boolean onCreate() { return true; }
    @Override public String getType(Uri uri) { return "text/html"; }
    @Override public Cursor query(Uri uri, String[] projection, String selection, String[] args, String order) {
        MatrixCursor result = new MatrixCursor(new String[] {OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE});
        result.addRow(new Object[] {"M7 benchmark.html", 0L});
        return result;
    }
    @Override public ParcelFileDescriptor openFile(Uri uri, String mode) throws FileNotFoundException {
        if (!mode.equals("r") || !uri.getPath().equals("/reader")) throw new FileNotFoundException("Read only fixture");
        File file = new File(getContext().getCacheDir(), "benchmark.html");
        StringBuilder html = new StringBuilder("<title>M7 Benchmark Reader</title><h1>Release performance journey</h1>");
        for (int i = 0; i < 180; i++) html.append("<p>Paragraph ").append(i).append(
            ". The lighthouse keeper walks along the harbour and watches the changing tide. This deterministic open fixture exercises canonical page turns and Compose paragraph rendering.</p>");
        try (FileOutputStream output = new FileOutputStream(file)) {
            output.write(html.toString().getBytes(StandardCharsets.UTF_8));
        } catch (IOException error) { throw new FileNotFoundException(error.toString()); }
        return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY);
    }
    @Override public Uri insert(Uri uri, ContentValues values) { throw new UnsupportedOperationException(); }
    @Override public int update(Uri uri, ContentValues values, String selection, String[] args) { throw new UnsupportedOperationException(); }
    @Override public int delete(Uri uri, String selection, String[] args) { throw new UnsupportedOperationException(); }
}
