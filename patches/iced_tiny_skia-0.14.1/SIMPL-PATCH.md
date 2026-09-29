# Control and rounded-edge clipping

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

Paragraph, editor, raw text, path geometry and image rendering remain upstream.
These changes may add mask work for cached
control glyphs; the settings damage grouping fix removes far more repeated work.
No font or image data, GPU backend or runtime dependency is added.

The retained application probe checks incremental CPU painting against a full
redraw at 100%, 150%, 200% DPI and in a small window with the language picker.
See [the damage patch](../iced_graphics-0.14.0/SIMPL-PATCH.md) for the command and
measurement boundaries. Replace this override only after those cases pass.
