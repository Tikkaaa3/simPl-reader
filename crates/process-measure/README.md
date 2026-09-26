# process-measure

`process-measure` is an optional Windows resource CLI, not a product prerequisite. It starts one local executable directly (never through a shell), places the suspended process in a private kill-on-close Job Object, samples the root process, and writes a versioned manifest plus streamed JSONL. It is not product telemetry or a reader benchmark.

## Build and run

Use the repository's initialized x64 MSVC environment (root `README.md`), then:

```powershell
cargo build -p process-measure --release --locked --offline
New-Item -ItemType Directory -Force target\measure-runs | Out-Null
.\target\release\process-measure.exe `
  --out target\measure-runs\idle-1 `
  --interval-ms 250 --duration-ms 10000 `
  --declare source_revision=abc123 --declare build_profile=release `
  -- C:\absolute\path\reader.exe --reader-option "value with spaces"
```

`--out` must name a new directory whose parent already exists (the example creates that parent). Defaults are 250 ms and 10 s. Intervals are at most one hour and durations at most 24 hours; duration must be at least one interval. `--cwd`, `--stdio null|inherit|file`, and repeated `--declare key=value` are optional. `null` is the default stdio policy; `file` streams to `target.stdout` / `target.stderr` without RAM capture. Options after `--` are target arguments verbatim. Arguments and paths are recorded, so do not put secrets in them.

The executable and working directory are canonicalized before launch. `.cmd` and `.bat` wrappers are rejected. Windows' standard `CreateProcessW` argument quoting preserves Unicode, empty arguments, embedded quotes, and trailing backslashes; the target's own parser must use the normal Windows/MSVC convention. No shell, PowerShell, WMI, or per-sample helper is launched.

## Artifacts (`process-measure/v1`)

* `manifest.json` is written as `in_progress` before launch and replaced through a flushed temporary file. `state`, `collection_stop_reason`, data-only `collection_valid`, independent `target_exit_code`, `tool_exit_success`, harness/cleanup errors, and counts are separate. A nonzero target can have a valid collection while the CLI correctly exits nonzero. Forced sampler termination may leave `in_progress`; storage failure may leave the prior manifest or `.tmp`, and a final partial JSONL row must be ignored by consumers.
* `samples.jsonl` contains one independently parseable object per completed row and is flushed after each row. Each metric is tagged `valid`, `unavailable`, `unsupported`, or `terminal_unavailable`; a genuine numeric zero is never confused with query failure.

Raw target fields are root-only:

| field | unit/source |
|---|---|
| `user_cpu_100ns`, `kernel_cpu_100ns` | cumulative `GetProcessTimes`, 100 ns |
| `private_working_set_bytes` | `PROCESS_MEMORY_COUNTERS_EX2.PrivateWorkingSetSize` |
| `private_commit_bytes` | `PROCESS_MEMORY_COUNTERS_EX2.PrivateUsage` |
| `total_working_set_bytes` | additional diagnostic, `WorkingSetSize`; not a substitute |

Each row retains scheduled/start/CPU-observation/end QPC ticks, schedule slip, query span, and sequence. Queries are sequential, not atomic. Derived CPU uses consecutive valid observations and actual QPC elapsed time:

`one_logical_percent = 100 * (delta_user_100ns + delta_kernel_100ns) / elapsed_100ns`

`machine_percent = one_logical_percent / GetActiveProcessorCount(ALL_PROCESSOR_GROUPS)`

One-logical values may exceed 100 and are not clamped. First, failed, regressing, overflowing, or non-positive-clock observations carry explicit reasons. A delta spanning an invalid target row is labeled. `sampler_delta_*` is the sampler's CPU delta between consecutive successful sampler observations; a failed sampler observation clears that baseline, so no interval is silently bridged. Manifest `sampler_cpu_delta_*` covers only the measured run-control window from immediately before launch through post-cleanup queries. It excludes pre-launch hashing/environment collection and final manifest serialization, so it is not total process-lifetime cost. Sampler cost is neither added to nor subtracted from target values.

Waiting is bounded by root exit, the absolute lifetime deadline (started before `CreateProcessW`), the next sample, and a 50 ms interrupt-response slice. Late sampling advances to the first future nominal slot; missed slots are not emitted as catch-up rows. Periodic Job Object PID snapshots and the run identity summary are each capped at 256 identities. Per-row status and manifest truncation/failure counters make snapshot degradation explicit; short-lived helpers can still be missed. Helpers are never aggregated into root metrics. Root exit closes and checks the remaining workload tree; duration/interrupt termination is intentional. Kill-on-close protects descendants if the sampler dies abruptly. `STARTUPINFOEXW` supplies an explicit selected-stdio handle list; arbitrary ambient inheritable handles, Job/process/sampler handles are excluded.

Exit 0 means at least two valid CPU observations and one valid observation of both required private-memory metrics, no required live-query/output/cleanup failure, and either natural target exit 0 or intentional duration termination. Any required live-query failure invalidates collection even if other rows are valid; expected terminal memory loss does not. Interruption, launch/input/output failure, nonzero target exit, insufficient data, and required-metric failure are nonzero.

## Platform support and limitations

The private-working-set EX2 field is gated conservatively using the actual Windows build/UBR registry values: Windows 10 22H2 build 19045.3448+, Windows 11 22H2 build 22621.2283+, or later builds. Earlier/unknown hosts report `unsupported`, never a fabricated zero; private commit remains independently reported when available. This tool is validated on the current Windows 11 x64 host and does not establish the product's minimum Windows version.

QPC is monotonic sampling time. UTC FILETIME is run/process identity time. They are never subtracted. Native architecture comes from `GetNativeSystemInfo`; the available CPU identity string comes from `PROCESSOR_IDENTIFIER` and is explicitly labeled as ambient-environment provenance because callers can override it. Idle scheduling, working-set trimming, and memory accounting are noisy. The tool does not report ETW/presentation timing, GPU memory, renderer identity, frame pacing, recursive process totals, percentiles, or roadmap pass/fail judgments. Optional GPU/backend/power/display facts may be operator declarations and are not inferred.

## Controlled calibration recipe

The test child is disposable verification support, not a workload claim. From the repository root after a release build:

```powershell
New-Item -ItemType Directory -Force target\measure-runs | Out-Null
$tool = Resolve-Path .\target\release\process-measure.exe
$child = Resolve-Path .\target\release\process-measure-testchild.exe
& $tool --out target\measure-runs\idle   --interval-ms 100 --duration-ms 900 -- $child sleep 750
& $tool --out target\measure-runs\busy   --interval-ms 100 --duration-ms 900 -- $child busy 750
& $tool --out target\measure-runs\reserve --interval-ms 100 --duration-ms 900 -- $child memory reserve 67108864 750
& $tool --out target\measure-runs\commit  --interval-ms 100 --duration-ms 900 -- $child memory commit 67108864 750
& $tool --out target\measure-runs\touch   --interval-ms 100 --duration-ms 900 -- $child memory touch 67108864 750
```

Other test-only modes are `exit <code>`, `sleep-exit <ms> <code>`, `echo ...`, and `helper-root <helper-ms> <root-hold-ms> <marker>`. Memory mode reserves address space; `commit` commits without touching every page; `touch` writes one byte per page and holds it.

For each pair of consecutive valid rows, independently recompute:

```text
delta_cpu = (user[n]-user[n-1]) + (kernel[n]-kernel[n-1])
elapsed_100ns = round((qpc[n]-qpc[n-1]) * 10,000,000 / qpc_frequency_hz)
one_logical = 100 * delta_cpu / elapsed_100ns
machine = one_logical / environment.logical_processor_count.value
```

During a long `touch` hold, an independent PowerShell cross-check is:

```powershell
$m = Get-Content -Raw target\measure-runs\touch\manifest.json | ConvertFrom-Json
Get-Process -Id $m.target_pid | Select-Object Id,PrivateMemorySize64,WorkingSet64
```

`PrivateMemorySize64` should be compared as an independent private-commit observation; `WorkingSet64` is total working set and must not be relabeled private working set. Expect noise, not exact equality. Inspect row `sample_end_qpc-sample_start_qpc`, schedule slip, `sampler_delta_*`, and manifest sampler-window deltas without applying a roadmap budget.

## Dependencies and licenses

Direct versions are pinned in `Cargo.toml`; the full graph is in `Cargo.lock`.

* `windows-sys 0.61.2` (MIT OR Apache-2.0): maintained Microsoft raw Win32 binding, with only call-site feature modules enabled. Chosen over hand-written FFI and a broad monitoring framework. Unsafe handle/buffer boundaries remain localized in `job`, `win`, and the BCrypt helper.
* `serde 1.0.229` (MIT OR Apache-2.0) with `derive`, and `serde_json 1.0.151` (MIT OR Apache-2.0): versioned schema and JSON/JSONL encoding. Chosen because a custom JSON encoder is explicitly unsafe for this artifact contract. Lockfile transitives are `itoa`, `memchr`, `proc-macro2`, `quote`, `syn`, `unicode-ident`, `windows-link`, and `zmij`; inspect `cargo metadata --locked --offline` / local registry license files for the locked graph.

These dependencies serve a separate executable and do not enter the reader's dependency graph. Win32 launch/query and JSON artifacts are the tool's purpose; no universal profiler interface or product adapter is needed. No dependency here starts background threads or performs network I/O. Incremental binary-size/RAM deltas were not isolated.

When packages are absent from Cargo's cache, provision them with:

```powershell
cargo fetch --locked
```

`--offline` is optional after provisioning, not a development authorization rule. No async runtime, CLI framework, crypto implementation, or monitoring framework is used; SHA-256 uses Windows CNG BCrypt.

## API references

* [GetProcessTimes](https://learn.microsoft.com/windows/win32/api/processthreadsapi/nf-processthreadsapi-getprocesstimes)
* [GetProcessMemoryInfo](https://learn.microsoft.com/windows/win32/api/psapi/nf-psapi-getprocessmemoryinfo)
* [PROCESS_MEMORY_COUNTERS_EX2](https://learn.microsoft.com/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex2)
* [Job Objects / kill-on-close](https://learn.microsoft.com/windows/win32/api/jobapi2/nf-jobapi2-assignprocesstojobobject)
* [Windows C/C++ argument parsing](https://learn.microsoft.com/cpp/c-language/parsing-c-command-line-arguments)
