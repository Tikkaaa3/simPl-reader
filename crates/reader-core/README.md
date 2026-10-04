# Shared reader behavior

`read_aloud` contains platform-independent speech rules. Windows SAPI and
Android TTS share UTF-16-to-source-byte conversion, conservative language
guessing, sentence starts and page-span lookup. Android also uses bounded
3500-unit chunks and section/row cursors through the UniFFI speech plan.

The crate has no UI, operating-system, speech-engine or runtime dependencies.
Platform adapters retain their own voice and playback lifecycle. The language
guess is a hint for sufficiently long Latin text, not a language detector.
