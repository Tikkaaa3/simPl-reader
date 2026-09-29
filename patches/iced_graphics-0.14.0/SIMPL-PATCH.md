# Contained damage regions

This is the crates.io `iced_graphics` 0.14.0 package, with its upstream font
assets, package metadata and MIT license. Original package SHA-256:
`234ca1c2cec4155055f68fa5fad1b5242c496ac8238d80a259bca382fb44a102`.
Upstream commit: `3997291f318a8bc06fa522f5579836fb3feb94df`, `graphics/`.

Only `src/damage.rs` changes runtime behavior: grouped regions wholly covered
by another group are discarded. The existing proximity/area grouping policy,
clipping and dirty-pixel coverage remain the same. Separate regions remain
separate. This avoids painting the same pixels and intersecting reader layers
many times when a large scrolling surface is emitted after small text groups.

The settings content has a moving opaque fill matching its panel. Together,
these changes produce one clipped damage region per settings scroll step.
Without the fill, many disjoint text regions can still trigger repeated CPU
compositing. Removing either change can restore the settings regression.

The region checks scan already emitted groups; cost is quadratic in the number
of retained groups in the worst case, not in the number of source glyphs. The
normal proximity grouping runs first. No surface-sized cache is introduced.

Verify the generic coverage cases with:

```powershell
cargo test -p iced_graphics --lib damage::tests --locked --offline
```

The application also tests real settings widget damage without timing thresholds.
For CPU timings and full-versus-incremental pixel comparisons:

```powershell
$env:SIMPL_PREVIEW_OUTPUT = 'target/settings-scroll-previews'
cargo test --release -p iced-shell --bin iced-shell profile_settings_scroll --locked --offline -- --ignored --nocapture
```

This probe maintains the production widget tree and renderer between frames and
uses the same layer diff/group operations as the Windows CPU compositor. It
models a buffer age of one. It excludes native window presentation and OS input;
results are CPU work measurements, not displayed-frame latency or FPS guarantees.

Measured on the development machine on 2026-09-30: 1280×800 logical window,
synthetic book, 70 persistent frames scrolling down/up by 12 logical pixels,
first 10 discarded. Median total CPU time went from 136.45 to 10.86 ms at
100% scale and from 265.53 to 22.29 ms at 150%. The shelf case went from 52.93
to 6.56 ms. The resulting damage group count is one per moving frame.
The full-redraw pixel comparisons also pass at 200%, in the dark theme and
with an expanded language picker in a 640×480 window at 150%.
