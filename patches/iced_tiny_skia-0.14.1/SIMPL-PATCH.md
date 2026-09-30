# Control clipping and CPU painting cost

This is the crates.io `iced_tiny_skia` 0.14.1 package with its upstream metadata
and MIT license. Original package SHA-256:
`c267596d742714b1853cc10c3983a367762816fc4836bd3b79f76ce76787d6f8`.
Upstream commit: `0ecf60664df7b8ac7d7aef5f7279d5323027f693`, `tiny_skia/`.

The `Text::Cached` branch in `src/engine.rs`
enforces the intersection of the local text clip and layer/damage clip. Upstream
used the local clip rectangle as if it bounded the glyphs; identical clips then
disabled masking. A voice pick-list arrow scrolled partly beyond the settings
viewport could draw into the panel padding and leave a stale arrow after scrolling.

Cached text now always uses the intersection mask. Rounded quads also retain
their layer mask when their nominal bounds lie within one physical pixel of
the clip edge. Their anti-aliasing can otherwise paint beyond those bounds,
leaving faint trails outside the scrolling content, visible in the dark theme.

The engine also remembers the last clip bounds within each `Renderer::draw`.
Repeated identical clips reuse the existing mask instead of clearing and filling
the entire viewport mask again. All shared-mask adjustments go through the engine;
each draw resets the remembered bounds because callers may replace or mutate
their mask between draws. Changed bounds still rebuild the mask. This preserves
the clipping fix while reducing repeated work for adjacent control glyphs.

The compositor also skips an earlier layer when a later opaque color quad fully
covers the damaged area inside its clip. Its coverage test insets rounded corners
and borders, including one physical pixel for antialiasing; transparent or partial
coverage retains the original drawing path. Square solid-color quads without a
border or shadow use `fill_rect` only when every physical edge is integer-aligned.
Fractional edges keep the upstream path to preserve pixel rounding.

Paragraph, editor, raw text, geometry and image rendering semantics remain upstream.
No font or image data, GPU backend or runtime dependency is added. The mask
regression checks fractional bounds, repeated clips, shrinking/expanding clips
and a fresh differently sized mask after a draw boundary. Two additional tests
compare optimized painting against the original path pixel-for-pixel at
100/125/150/200% scale, including alpha, fractional coordinates, clip masks,
rounded corners and fully/partly covered layers:

```powershell
cargo test -p iced_tiny_skia --lib --offline --locked
```

The retained application probe checks incremental CPU painting against a full
redraw at 100%, 150%, 200% DPI and in a small window with the language picker.
See [the damage patch](../iced_graphics-0.14.0/SIMPL-PATCH.md) for the command and
measurement boundaries. Replace this override only after those cases pass.
The [reader comfort follow-up](../../reader-comfort-review-2026-09-30.md) records
the latest measured improvement and the remaining 200% frame-time limitation.
