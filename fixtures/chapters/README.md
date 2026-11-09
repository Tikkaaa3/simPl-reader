# Three Rooms

A project-authored EPUB with three chapters of 14 numbered paragraphs each, for
live checks that cross a chapter boundary. The demo books from
`scripts/promotion-fixtures.py` each have a single chapter.

```powershell
python -I fixtures/chapters/build_epub.py
```

It writes `target/lane-fixtures/Three Rooms.epub`, or the path given as the
first argument. The window title names the current chapter.
