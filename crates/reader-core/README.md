# Shared reader behavior

`word_translation` contains dictionary languages, supported directions, settings
validation, Unicode word boundaries, bounded NFKC query normalization and the
sorted TSV lexicon. Exact entries precede conservative English base forms, which
remain labeled. It performs no file or network I/O. `reader-document` retains the
existing pinned package catalog, SHA-256/ZIP validation, atomic installation and
single-direction cache; both the Windows adapter and Android UniFFI use it.

`read_aloud` contains platform-independent speech rules. Windows SAPI and
Android TTS share UTF-16-to-source-byte conversion, conservative language
guessing, sentence starts and page-span lookup. Android also uses bounded
3500-unit chunks and section/row cursors through the UniFFI speech plan.

The crate has no UI, operating-system, speech-engine or runtime dependencies.
Platform adapters retain their own voice and playback lifecycle. The language
guess is a hint for sufficiently long Latin text, not a language detector.
