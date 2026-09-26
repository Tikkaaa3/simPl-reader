# Interaction analyzer regression input

This is a numeric subset of the `1000-scroll-800-1` run from the local
2026-09-24 round-2 interaction experiment. `app.csv` preserves the original
callback trace. The JSON files retain only the fields consumed by
`tests/analyze-interaction.py`; unused machine paths, source manifests and
screenshot references were removed. Capture hashes are historical metadata,
not a claim that the original images are distributed here.

The original run used fixture revision `reader-workload-fx-2`. The current
reader uses `reader-workload-fx-3`. Do not use this subset as a benchmark of
the current build, a complete experiment record, or displayed-frame evidence.
It exists to check that syntactically valid callbacks injected into a recorded
idle interval are rejected. The complete original records were archived
outside the repository before the English-only publication cleanup.

Run from the repository root:

```powershell
python crates/iced-shell/tests/analyze-interaction-tests.py
```
