# Bundled offline word dictionaries

The dictionary data in `words.zip` and the adapted indexes are licensed under
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

No endorsement of simPl by these projects or contributors is implied.
Original source URLs, SHA-256 checksums, resulting index checksums and entry
counts are recorded in `manifest.json`, also included inside the archive.

## Adaptations

simPl's build script combines records by Unicode-normalized, case-insensitive
headword; handles Turkish dotted/dotless I; replaces curly apostrophes; collapses
whitespace; removes duplicate meanings; and stores bounded plain-text TSV indexes
in a deterministic ZIP archive. Meanings are limited to 20 per entry and 1024
UTF-8 bytes each; headword keys to 256 bytes. Korean glosses are extracted from
senses. Chinese simplified and traditional entries retain explicit source forms.
The adapted dictionary data remains CC BY-SA 4.0.

The application embeds the archive and decompresses only the active pair.
Packaged releases include the notices and manifest beside the executable;
`words.zip` and its builder are available under `assets/dictionaries/` and
`scripts/` in the source repository. The data may be copied, adapted and
redistributed, including commercially, under CC BY-SA 4.0.
It shows up to eight meanings per result. Available words and translation quality
vary by pair; definitions are dictionary results, not sentence translations.
No network request, Python installation, SQLite library or model is needed at runtime.

To rebuild, run `python scripts/build-dictionaries.py` from the repository.
Python's standard library is sufficient. Downloads are cached in
`target/dictionary-research/`; keep the matching source files for exact reproduction.
Refreshing a moving upstream endpoint can change its checksum and results.
