# Downloadable offline word dictionaries

The dictionary data in `packs/*.zip` and the adapted indexes are licensed under
**Creative Commons Attribution-ShareAlike 4.0 International (CC BY-SA 4.0)**:
https://creativecommons.org/licenses/by-sa/4.0/
The complete legal text is in `CC-BY-SA-4.0.txt`.
simPl's source and binary terms do not restrict the rights granted for this data.

## Attribution and sources

- **WikDict**, by Karl Bartel: https://www.wikdict.com/page/about
  Data by **Wiktionary contributors**, extracted through **DBnary**:
  https://www.wiktionary.org/ and https://kaiko.getalp.org/about-dbnary/
  The twelve English ↔ Turkish, Spanish, German, French, Japanese and Chinese
  indexes use the WikDict SQLite snapshot `2_2026-06`.
- **Kaikki / Wiktextract**, by Tatu Ylonen and contributors:
  https://kaikki.org/dictionary/Korean/index.html
  Korean → English meanings come from **English Wiktionary contributors**:
  https://en.wiktionary.org/wiki/Wiktionary:Copyrights
  The English-Wiktionary Korean JSONL subset was downloaded on 2026-09-30.
  Its upstream endpoint is deprecated; the packaged data works independently.
- **CC-CEDICT contributors / MDBG**, continuing Paul Denisowski's CEDICT project:
  https://www.mdbg.net/chinese/dictionary?page=cedict
  CC-CEDICT supplements Chinese → English with explicit simplified and traditional
  headwords. Snapshot downloaded on 2026-09-30.
  Retained notice from the downloaded CC-CEDICT source header:
  **CEDICT - Copyright (C) 1997, 1998 Paul Andrew Denisowski**.
  Source download: https://www.mdbg.net/chinese/export/cedict/cedict_1_0_ts_utf-8_mdbg.txt.gz

No endorsement of simPl by these projects or contributors is implied.
The data is provided as-is, without warranties; see the complete CC BY-SA 4.0
legal text for the disclaimer and limitation of liability.
Original source URLs, SHA-256 checksums, resulting index checksums and entry
counts are recorded in `manifest.json`. Each package includes its own manifest,
this attribution and the complete license. `catalog.json` records download sizes
and package/index SHA-256 hashes for the immutable data release.

## Adaptations

simPl's build script combines records by Unicode-normalized, case-insensitive
headword; handles Turkish dotted/dotless I; replaces curly apostrophes; collapses
whitespace; removes duplicate meanings; and stores bounded plain-text TSV indexes
in a deterministic ZIP archive. Meanings are limited to 20 per entry and 1024
UTF-8 bytes each; headword keys to 256 bytes. Korean glosses are extracted from
senses. Chinese simplified and traditional entries retain explicit source forms.
The adapted dictionary data remains CC BY-SA 4.0.

The application embeds only the small package catalog. No word data is included
in its executable or normal portable/installer package. Users explicitly download
their chosen language direction from the dictionary data release, or import the
same ZIP file in Settings. Files are kept under `%LOCALAPPDATA%\simPl\dictionaries`
and work offline after installation. Only the active direction is decompressed.
Downloads transmit no selected text or document content; HTTPS is used only
to fetch the selected ZIP. Packages and their builders are available under
`assets/dictionaries/packs/` and `scripts/` in the source repository.
The data may be copied, adapted and
redistributed, including commercially, under CC BY-SA 4.0.
It shows up to eight meanings per result. Available words and translation quality
vary by pair; definitions are dictionary results, not sentence translations.
No Python installation, SQLite library or model is needed at runtime.

To rebuild, run `python scripts/build-dictionaries.py` from the repository.
The script then packages each direction independently. Python's standard library
is sufficient. Source downloads are cached in
`target/dictionary-research/`; keep the matching source files for exact reproduction.
Refreshing a moving upstream endpoint can change its checksum and results.
Before packaging changed data or notices, choose a new `VERSION` in
`scripts/package-dictionaries.py` and publish that version's assets and catalog.
The packager refuses to replace existing ZIPs with different bytes.

## Notice clarification for existing downloads

This updated attribution also applies to the unchanged dictionary packages in
the `dictionaries-v1-2026-09-30` release. In particular, it retains the original
CEDICT copyright notice for the Chinese → English supplement. Published ZIPs
and their catalog hashes remain unchanged; this notice supplements the README
already included in them. When redistributing those packages, include this
updated notice alongside them. Changed package bytes require a new data release
and matching application catalog, rather than replacing the existing ZIPs.
