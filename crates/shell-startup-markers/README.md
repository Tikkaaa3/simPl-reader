# shell-startup-markers

`shell-startup-markers` is the small timing library used by `iced-shell`.
It owns the bounded, QPC-only startup-marker stream format
`shell-startup-markers/v1` — one UTF-8 line per marker,
`shell-startup-markers/v1 <event> <qpc_ticks>\n`.

It provides:

- `encode_line` / `parse`: the format and its validating parser (malformed,
  unknown, or QPC-regressing lines are preserved as labeled evidence, never
  dropped or fabricated).
- `qpc` / `qpc_frequency`: best-effort raw `QueryPerformanceCounter` /
  `QueryPerformanceFrequency` helpers using the system-wide monotonic clock.
- `Emitter` / `emitter_from_env`: the process-scoped opt-in emitter. The gate
  variable (`ICED_SHELL_STARTUP_MARKERS`) must
  be set for the specific process by its launcher; when it is absent, no file
  is created and shell behavior is unchanged. Each event kind is emitted at
  most once per process (bounded by construction), and a failed marker
  stream degrades to "markers absent" rather than changing normal behavior.

Events: `process_entry` (before any framework work), `window_created`
(the framework created the window), `render_submitted` (the shell's first
render submission — app-declared supporting evidence only, never the
displayed-frame endpoint), `redraw_requested` (framework-declared redraw,
Iced only). Marker lines contain QPC ticks only; UTC FILETIME never appears.

App-declared markers are supporting evidence, not a first-visible-frame
measurement. The old cross-framework observer has been removed; this library
remains because the prototype also uses its QPC helpers for interaction traces.

Dependency: only the already workspace-pinned `windows-sys 0.61.2`
(MIT OR Apache-2.0), feature-gated to `Win32_System_Performance`
(`QueryPerformanceCounter`/`QueryPerformanceFrequency`); no new third-party
packages were introduced by this task.
