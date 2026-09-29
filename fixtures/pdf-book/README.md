# PDF Book regression inputs

`build_fixtures.py` authors five QA PDFs under `target/book-milestone3/fixtures/`.
They are diagnostic inputs, not application assets.

```powershell
python -m venv target\pdf-book-fixture-env
& target\pdf-book-fixture-env\Scripts\python.exe -m pip install reportlab pypdf
& target\pdf-book-fixture-env\Scripts\python.exe fixtures\pdf-book\build_fixtures.py
```

| Input | Expected behavior |
| --- | --- |
| `prose.pdf` | Four text pages reconstruct in source order; the fifth preserves its illustration. All five source pages remain addressable. |
| `columns.pdf` | Ambiguous column order retains the page slot with a Document-view explanation. The source PDF remains readable. |
| `scanned.pdf` | The illustration is retained; no OCR text is invented. |
| `restricted.pdf` | Opens without a password, but extraction permission is denied and Book conversion fails. |
| `layout.pdf` | Centered title, Contents rows with a same-document link, bold/italic numbered entries, Roman printed and PDF page labels, and a complex table page using original-page fallback. |

See the [developer guide](../../crates/iced-shell/README.md#pdf-book-conversion-and-qa)
for native rendering and isolated persistence checks. Permission-denial checks use
`permissions` as the expected error substring. Columns and image-only pages are
page-level cases, not whole-document conversion failures.

The prose and builder are project-authored. The illustration comes from
`fixtures/reader-workload/assets/images/reader-sample.png`; its
[authorship notice](../reader-workload/licenses/PROJECT-AUTHORED-NOTICE.txt)
is retained. No third-party book text is embedded here. ReportLab and pypdf are
optional QA dependencies, not application dependencies.

The `render_pdf_book_previews` ignored test additionally uses the 111-page
[Planet eBook Alice PDF](https://www.planetebook.com/free-ebooks/alices-adventures-in-wonderland.pdf).
That download and generated screenshots stay in `target/`; they are not bundled.
The separate cache/blank-page test uses the locally supplied Pride and Prejudice
and Sherlock Holmes editions described in the developer guide.
