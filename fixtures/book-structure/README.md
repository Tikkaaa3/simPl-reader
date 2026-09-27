# Structured Book fixture

Project-authored HTML covering nested/numbered lists, quotations, inline emphasis,
preformatted whitespace, scene breaks, tables, MathML fallbacks, an image/caption,
footnotes, Unicode, RTL links and missing/external targets. `illustration.png` is
the existing project-authored `reader-workload/assets/images/reader-sample.png`;
see that fixture's `licenses/PROJECT-AUTHORED-NOTICE.txt`.

Open `structured.html` in the normal reader. Generate the EPUB with:

```powershell
python fixtures/book-structure/build_epub.py
```

It writes `target/book-milestone2/structured.epub`. Its note link targets a
`linear="no"` supplementary section; Back / Alt+Left should restore the passage.
The production widget preview test also accepts these two paths through
`SIMPL_PREVIEW_HTML` / `SIMPL_PREVIEW_EPUB`. Set `SIMPL_PREVIEW_ROW=12` for formulas
and the illustration, or `SIMPL_PREVIEW_CHAPTER=1` for the auxiliary EPUB note.
Generated EPUBs and PNG previews stay under `target/`; none of this input is
required or bundled by the normal reader.
