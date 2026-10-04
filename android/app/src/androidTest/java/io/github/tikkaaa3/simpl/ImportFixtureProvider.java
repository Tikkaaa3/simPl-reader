package io.github.tikkaaa3.simpl;

import android.database.Cursor;
import android.content.Intent;
import android.database.MatrixCursor;
import android.graphics.Bitmap;
import android.graphics.Color;
import android.os.CancellationSignal;
import android.os.ParcelFileDescriptor;
import android.provider.DocumentsContract.Document;
import android.provider.DocumentsContract;
import android.provider.DocumentsContract.Root;
import android.provider.DocumentsProvider;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.LinkedHashMap;
import java.util.Locale;
import java.util.Map;
import java.util.zip.CRC32;
import java.util.zip.ZipEntry;
import java.util.zip.ZipOutputStream;

/** Standalone test-APK provider: it must not depend on the target APK's Kotlin runtime. */
public class ImportFixtureProvider extends DocumentsProvider {
    private static final String[] COLUMNS = {Document.COLUMN_DOCUMENT_ID, Document.COLUMN_DISPLAY_NAME,
        Document.COLUMN_MIME_TYPE, Document.COLUMN_FLAGS, Document.COLUMN_SIZE};
    private final Map<String, String> files = new LinkedHashMap<>();

    public ImportFixtureProvider() {
        files.put("Notes.txt", "text/plain"); files.put("Guide.md", "text/markdown");
        files.put("Harbour.epub", "application/epub+zip"); files.put("Tides.pdf", "application/pdf");
        files.put("chapter.html", "text/html"); files.put("bad.epub", "application/epub+zip");
        files.put("images/cover.png", "image/png");
        files.put("Dictionary.zip", "application/zip"); files.put("bad.zip", "application/zip");
    }
    @Override public boolean onCreate() {
        return true;
    }
    public static void grant(android.content.Context context) {
        int flags = Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION;
        for (String id : new String[] {"Notes.txt", "Guide.md", "Harbour.epub", "Tides.pdf", "chapter.html", "bad.epub", "images/cover.png", "Dictionary.zip", "bad.zip"}) context.grantUriPermission("io.github.tikkaaa3.simpl",
            DocumentsContract.buildDocumentUri("io.github.tikkaaa3.simpl.test.documents", id), flags);
        context.grantUriPermission("io.github.tikkaaa3.simpl",
            DocumentsContract.buildTreeDocumentUri("io.github.tikkaaa3.simpl.test.documents", "root"), flags | Intent.FLAG_GRANT_PREFIX_URI_PERMISSION);
    }
    @Override public Cursor queryRoots(String[] projection) {
        MatrixCursor cursor = new MatrixCursor(projection != null ? projection : new String[] {
            Root.COLUMN_ROOT_ID, Root.COLUMN_DOCUMENT_ID, Root.COLUMN_TITLE, Root.COLUMN_FLAGS, Root.COLUMN_MIME_TYPES});
        Map<String, Object> row = new LinkedHashMap<>();
        row.put(Root.COLUMN_ROOT_ID, "root"); row.put(Root.COLUMN_DOCUMENT_ID, "root");
        row.put(Root.COLUMN_TITLE, "simPl test books"); row.put(Root.COLUMN_FLAGS, Root.FLAG_SUPPORTS_IS_CHILD);
        row.put(Root.COLUMN_MIME_TYPES, "*/*"); addRow(cursor, row); return cursor;
    }
    private static void addRow(MatrixCursor cursor, Map<String, Object> row) {
        MatrixCursor.RowBuilder builder = cursor.newRow();
        for (String column : cursor.getColumnNames()) builder.add(row.get(column));
    }
    private void document(MatrixCursor cursor, String id) {
        Map<String, Object> row = new LinkedHashMap<>();
        row.put(Document.COLUMN_DOCUMENT_ID, id); row.put(Document.COLUMN_DISPLAY_NAME,
            id.equals("Tides.pdf") ? "Tides.PDF" : id.substring(id.lastIndexOf('/') + 1));
        row.put(Document.COLUMN_MIME_TYPE, id.equals("root") || id.equals("images") ? Document.MIME_TYPE_DIR : files.get(id));
        row.put(Document.COLUMN_FLAGS, 0); row.put(Document.COLUMN_SIZE, 0L); addRow(cursor, row);
    }
    @Override public Cursor queryDocument(String id, String[] projection) {
        MatrixCursor cursor = new MatrixCursor(projection != null ? projection : COLUMNS); document(cursor, id); return cursor;
    }
    @Override public Cursor queryChildDocuments(String parent, String[] projection, String sort) {
        MatrixCursor cursor = new MatrixCursor(projection != null ? projection : COLUMNS);
        if (parent.equals("root")) {
            for (String id : files.keySet()) if (!id.contains("/")) document(cursor, id);
            document(cursor, "images");
        } else if (parent.equals("images")) document(cursor, "images/cover.png");
        return cursor;
    }
    @Override public boolean isChildDocument(String parent, String id) { return parent.equals("root") || id.startsWith(parent + "/"); }
    @Override public ParcelFileDescriptor openDocument(String id, String mode, CancellationSignal signal) throws java.io.FileNotFoundException {
        if (!mode.equals("r")) throw new java.io.FileNotFoundException("Read only");
        try {
            byte[] bytes;
            switch (id) {
                case "Notes.txt": bytes = utf8("M3 provider notes.\n\nA second paragraph from the provider."); break;
                case "Guide.md": bytes = utf8("# M3 provider guide\n\nA **Markdown** document."); break;
                case "Harbour.epub": bytes = epub(); break;
                case "Tides.pdf": bytes = pdf(); break;
                case "chapter.html": bytes = utf8("<title>M3 provider chapter</title><meta name=author content='M3 author'><h1>M3 provider chapter</h1><p>Provider HTML.</p><img src='images/cover.png'>"); break;
                case "images/cover.png":
                    ByteArrayOutputStream image = new ByteArrayOutputStream();
                    Bitmap bitmap = Bitmap.createBitmap(120, 180, Bitmap.Config.ARGB_8888);
                    bitmap.eraseColor(Color.rgb(35, 80, 130)); bitmap.compress(Bitmap.CompressFormat.PNG, 100, image); bitmap.recycle();
                    bytes = image.toByteArray(); break;
                case "bad.epub": bytes = utf8("Broken EPUB"); break;
                case "bad.zip": bytes = utf8("Broken dictionary ZIP"); break;
                case "Dictionary.zip":
                    try (java.io.InputStream input = getContext().getAssets().open("en-tr-2026-09-30.zip")) {
                        ByteArrayOutputStream copied = new ByteArrayOutputStream(); byte[] buffer = new byte[32768]; int count;
                        while ((count = input.read(buffer)) >= 0) copied.write(buffer, 0, count);
                        bytes = copied.toByteArray();
                    }
                    break;
                default: throw new java.io.FileNotFoundException(id);
            }
            File file = new File(getContext().getCacheDir(), "provider-" + id.replace('/', '-'));
            try (FileOutputStream out = new FileOutputStream(file)) { out.write(bytes); }
            return ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY);
        } catch (IOException error) { throw new java.io.FileNotFoundException(error.toString()); }
    }
    private static byte[] utf8(String text) { return text.getBytes(StandardCharsets.UTF_8); }
    private static byte[] epub() throws IOException {
        Map<String, String> entries = new LinkedHashMap<>();
        entries.put("META-INF/container.xml", "<container xmlns='urn:oasis:names:tc:opendocument:xmlns:container' version='1.0'><rootfiles><rootfile full-path='OPS/book.opf' media-type='application/oebps-package+xml'/></rootfiles></container>");
        entries.put("OPS/book.opf", "<package xmlns='http://www.idpf.org/2007/opf' version='3.0'><metadata xmlns:dc='http://purl.org/dc/elements/1.1/'><dc:title>Harbour Lights</dc:title><dc:creator>simPl test</dc:creator></metadata><manifest><item id='one' href='one.xhtml' media-type='application/xhtml+xml'/></manifest><spine><itemref idref='one'/></spine></package>");
        entries.put("OPS/one.xhtml", "<html xmlns='http://www.w3.org/1999/xhtml'><body><h1>Harbour</h1><p>M3 provider EPUB.</p></body></html>");
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        try (ZipOutputStream zip = new ZipOutputStream(bytes)) {
            byte[] mime = utf8("application/epub+zip"); CRC32 crc = new CRC32(); crc.update(mime);
            ZipEntry first = new ZipEntry("mimetype"); first.setMethod(ZipEntry.STORED); first.setSize(mime.length); first.setCrc(crc.getValue()); first.setTime(0);
            zip.putNextEntry(first); zip.write(mime); zip.closeEntry();
            for (Map.Entry<String, String> entry : entries.entrySet()) {
                ZipEntry member = new ZipEntry(entry.getKey()); member.setTime(0); zip.putNextEntry(member); zip.write(utf8(entry.getValue())); zip.closeEntry();
            }
        }
        return bytes.toByteArray();
    }
    private static byte[] pdf() {
        String stream = "BT /F1 18 Tf 20 150 Td (M3 provider tides) Tj ET";
        String[] objects = {"<</Type/Catalog/Pages 2 0 R>>", "<</Type/Pages/Kids[4 0 R]/Count 1>>",
            "<</Type/Font/Subtype/Type1/BaseFont/Helvetica>>", "<</Type/Page/Parent 2 0 R/MediaBox[0 0 300 200]/Contents 5 0 R/Resources<</Font<</F1 3 0 R>>>>>>",
            "<</Length " + stream.length() + ">>\nstream\n" + stream + "\nendstream"};
        StringBuilder result = new StringBuilder("%PDF-1.4\n"); int[] offsets = new int[objects.length];
        for (int i = 0; i < objects.length; i++) { offsets[i] = result.length(); result.append(i + 1).append(" 0 obj\n").append(objects[i]).append("\nendobj\n"); }
        int xref = result.length(); result.append("xref\n0 6\n0000000000 65535 f \n");
        for (int offset : offsets) result.append(String.format(Locale.ROOT, "%010d 00000 n \n", offset));
        result.append("trailer\n<</Root 1 0 R/Size 6>>\nstartxref\n").append(xref).append("\n%%EOF\n"); return utf8(result.toString());
    }
}
