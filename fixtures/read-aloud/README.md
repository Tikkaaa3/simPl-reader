# Offline speech regression fixture

`build_epub.py` authors `target/tts-qa.epub`: image-only, Turkish prose,
image-only and English prose chapters. The illustration reuses the authored
[book structure fixture](../book-structure/README.md); no book is bundled.

Run from the repository root, with the normal Windows build environment:

```powershell
python fixtures/read-aloud/build_epub.py
$env:SIMPL_TTS_EPUB = (Join-Path $PWD 'target/tts-qa.epub')
cargo test -p iced-shell --bin iced-shell native_read_aloud --locked -- --ignored
cargo test -p iced-shell --bin iced-shell installed_voices_speak --locked -- --ignored
cargo test -p iced-shell --bin iced-shell render_read_aloud_previews --locked -- --ignored
```

Native speech checks use SAPI volume zero. They exercise empty-chapter transitions,
language selection, selection/page reading, pause/resume, stale PDF replies and
closing a document. Previews draw the production widgets at 1280 and 540 pixels
wide into `target/tts-previews`, without OS input.
