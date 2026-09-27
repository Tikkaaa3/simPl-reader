---
name: simPl Calm Literary
colors:
  surface: '#10141a'
  surface-dim: '#10141a'
  surface-bright: '#353940'
  surface-container-lowest: '#0a0e14'
  surface-container-low: '#181c22'
  surface-container: '#1c2026'
  surface-container-high: '#262a31'
  surface-container-highest: '#31353c'
  on-surface: '#dfe2eb'
  on-surface-variant: '#c0c7d4'
  inverse-surface: '#dfe2eb'
  inverse-on-surface: '#2d3137'
  outline: '#8b919d'
  outline-variant: '#414752'
  surface-tint: '#a2c9ff'
  primary: '#a2c9ff'
  on-primary: '#00315c'
  primary-container: '#58a6ff'
  on-primary-container: '#003a6b'
  inverse-primary: '#0060aa'
  secondary: '#bec7d2'
  on-secondary: '#29313a'
  secondary-container: '#414a53'
  on-secondary-container: '#b0b9c4'
  tertiary: '#c1c7d0'
  on-tertiary: '#2b3138'
  tertiary-container: '#9da3ac'
  on-tertiary-container: '#343a41'
  error: '#ffb4ab'
  on-error: '#690005'
  error-container: '#93000a'
  on-error-container: '#ffdad6'
  primary-fixed: '#d3e4ff'
  primary-fixed-dim: '#a2c9ff'
  on-primary-fixed: '#001c38'
  on-primary-fixed-variant: '#004882'
  secondary-fixed: '#dae3ee'
  secondary-fixed-dim: '#bec7d2'
  on-secondary-fixed: '#141c24'
  on-secondary-fixed-variant: '#3f4850'
  tertiary-fixed: '#dde3ec'
  tertiary-fixed-dim: '#c1c7d0'
  on-tertiary-fixed: '#161c23'
  on-tertiary-fixed-variant: '#41474f'
  background: '#10141a'
  on-background: '#dfe2eb'
  surface-variant: '#31353c'
typography:
  display-lg:
    fontFamily: Literata
    fontSize: 40px
    fontWeight: '400'
    lineHeight: 52px
    letterSpacing: -0.02em
  display-lg-mobile:
    fontFamily: Literata
    fontSize: 30px
    fontWeight: '400'
    lineHeight: 40px
    letterSpacing: -0.015em
  headline-lg:
    fontFamily: Literata
    fontSize: 28px
    fontWeight: '400'
    lineHeight: 38px
    letterSpacing: -0.01em
  headline-md:
    fontFamily: Literata
    fontSize: 22px
    fontWeight: '500'
    lineHeight: 32px
    letterSpacing: -0.005em
  headline-sm:
    fontFamily: Inter
    fontSize: 16px
    fontWeight: '600'
    lineHeight: 24px
    letterSpacing: 0em
  body-xl:
    fontFamily: Literata
    fontSize: 20px
    fontWeight: '400'
    lineHeight: 34px
    letterSpacing: 0.005em
  body-lg:
    fontFamily: Literata
    fontSize: 18px
    fontWeight: '400'
    lineHeight: 30px
    letterSpacing: 0.005em
  body-md:
    fontFamily: Literata
    fontSize: 16px
    fontWeight: '400'
    lineHeight: 26px
    letterSpacing: 0.01em
  body-sm:
    fontFamily: Inter
    fontSize: 14px
    fontWeight: '400'
    lineHeight: 22px
    letterSpacing: 0em
  label-lg:
    fontFamily: Inter
    fontSize: 13px
    fontWeight: '500'
    lineHeight: 18px
    letterSpacing: 0.01em
  label-md:
    fontFamily: Inter
    fontSize: 12px
    fontWeight: '500'
    lineHeight: 16px
    letterSpacing: 0.02em
  label-sm:
    fontFamily: Inter
    fontSize: 11px
    fontWeight: '500'
    lineHeight: 14px
    letterSpacing: 0.04em
rounded:
  sm: 0.125rem
  DEFAULT: 0.25rem
  md: 0.375rem
  lg: 0.5rem
  xl: 0.75rem
  full: 9999px
spacing:
  gutter: 1.5rem
  gutter-desktop: 2.5rem
  margin: 1.5rem
  margin-desktop: 3.5rem
  space-xs: 0.25rem
  space-sm: 0.5rem
  space-md: 1rem
  space-lg: 1.5rem
  space-xl: 2.5rem
---

## Brand & Style

This design system embodies pure literary minimalism: a serene, distraction-free environment tailored for deep, continuous reading and document synthesis across desktop environments. Designed for thinkers, researchers, writers, and book lovers, the interface actively recedes into the background to elevate the written word above all chrome.

The aesthetic fuses modern editorial calm with the disciplined precision of refined code editors—stripped of utilitarian developer cliches. Surfaces evoke deep, matte obsidian and midnight slate; contrast ratios prioritize prolonged ocular comfort over harsh luminescence; and micro-interactions behave like silent physical transitions rather than digital interruptions. There are no garish toolbars, saturated badges, or cognitive clutter. Every interaction is quiet, considered, and effortless.

## Colors

The palette establishes an atmosphere of quiet visual permanence:
- **Canvas Base (`#0d1117`):** Deep, non-reflective obsidian slate serving as the primary void for reading viewports and window framing.
- **Surface Elevation (`#161b22`):** Calm raised slate used for contextual sidebars, reading drawers, floating command palettes, and utility sheets.
- **Primary Accent (`#58a6ff`):** Crisp ice blue, applied with strict restraint to active text selections, focused indicators, subtle progress pips, and current chapter markers.
- **Muted Structural Neutral (`#30363d`):** Low-contrast boundary tone for hair-thin dividers, unobtrusive card outlines, and active track borders.
- **Typography Tones:**
  - *Primary Body (`#e6edf3`):* Soft bone white, carefully calibrated to eliminate eye strain associated with harsh `#ffffff` on dark backgrounds.
  - *Secondary / Metadata (`#8b949e`):* Balanced slate grey for reading time remaining, footnotes, page pagination, and shelf taxonomy.
  - *Tertiary / Disabled (`#484f58`):* Quiet stone for passive borders and dismissed statuses.

## Typography

The type system blends classical book craftsmanship with contemporary digital legibility:
- **Literata** acts as the narrative voice for headlines, long-form EPUB/PDF texts, and primary reading contents. Built specifically for sustained digital reading, its organic terminals and generous x-height prevent fatigue.
- **Inter** handles structural user interface chrome, library navigation, table of contents hierarchy, search results, and metadata readouts. It provides clean, neutral contrast against the literary warmth of the serif body.
- Body sizes default to relaxed line-height ratios (~1.65–1.7x font size) with measure control constrained between 58 and 72 characters per line for optimal reading cadence.

## Layout & Spacing

The spatial model employs a generous fixed-measure column for the reading viewport and flexible side rails for supporting navigational elements.

- **Reading Chamber:** Constrained to a centered maximum reading column of 680px to 760px to preserve ideal line lengths. All chrome auto-hides during active reading or scrolling, reclaiming 100% of the viewport.
- **Library & Shelf View:** A clean 12-column responsive layout featuring spacious margins (`margin-desktop: 3.5rem`) and airy column gutters (`gutter-desktop: 2.5rem`), transitioning from compact grid book cards on narrow screens to expansive editorial covers on wide displays.
- **Breakpoints:**
  - *Compact / Mobile Window (< 768px):* Single-column stream; margins collapse to `1.5rem`, drawer panels tuck into full-bleed overlays.
  - *Standard Desktop (768px – 1280px):* Sidebar tucks into an icon rail or collapsible flyout; reader centered.
  - *Wide Desktop (> 1280px):* Multi-pane capability with persistent table of contents, dual-page layout options, and dedicated annotation gutters.

## Elevation & Depth

This design system avoids loud, diffuse drop-shadows and skeuomorphic bevels. Visual hierarchy is established through disciplined tonal tiering and subtle hairline boundaries:

- **Level 0 (Base Canvas):** Background tone `#0d1117`. Flat and matte.
- **Level 1 (Docked Shelves & Panels):** Subtle tonal step `#161b22`, delimited strictly by a 1px border of `#30363d` without shadow.
- **Level 2 (Floating Palettes & Menus):** Background `#161b22` enhanced with a quiet, whisper-soft perimeter shadow (`0 8px 24px rgba(0, 0, 0, 0.45)`) and a continuous `#30363d` rim border to guarantee crisp separation against underlying text.
- **Backdrop Frosting:** Ephemeral sheets (e.g., Quick Switcher / HUD reading sliders) apply a minimal `backdrop-filter: blur(12px)` over background content to preserve focus while remaining grounded in the current page context.

## Shapes

The shape vocabulary uses the `Soft` (0.25rem / 4px base radius) archetype. This preserves the dignified, quiet demeanor of physical books and architectural minimalism.

- Buttons, inputs, and list items feature 4px corner radii.
- Modals, floating popovers, and reading cards step up to 8px (`rounded-lg`) to soften their floating silhouette.
- Full pills are strictly reserved for reading progress scrubbers, page counter capsules, and contextual tag indicators.

## Components

- **Buttons:**
  - *Ghost / Quiet (Default):* Transparent background, `#8b949e` label; on hover, shifts to `#161b22` fill with `#e6edf3` text.
  - *Primary:* Subtle `#1f6feb` or `#58a6ff` tinted accent background (`rgba(88, 166, 255, 0.12)`) with `#58a6ff` text and crisp border. Never solid screaming blue.
- **Book & Document Cards:**
  - Minimalistic covers resting on `#161b22` surfaces with a 1px `#30363d` rim. Metadata displays format pill (EPUB/PDF), completion percentage, and last-read timestamp in `label-sm`.
- **Lists & Table of Contents:**
  - Borderless rows with generous vertical breathing room (`space-sm` to `space-md`). Active reading location marked by a slim 2px ice blue left indicator bar and elevated bone text color.
- **Inputs & Search Palettes:**
  - Recessed `#0d1117` background surrounded by 1px `#30363d` border. Focused state replaces border with clean `#58a6ff` without outer glow rings. Monospaced or clean sans glyphs for search queries.
- **Reading Progress & Scrubber:**
  - An ultra-slim 2px track along the bottom viewport edge in `#30363d`, filling with `#58a6ff`. Hover expands to 6px with chapter notch delimiters and time-remaining tooltips.
- **Text Selection & Annotations:**
  - Highlights use low-opacity washes (e.g., Ice Blue `rgba(88, 166, 255, 0.20)` or Warm Ochre `rgba(227, 179, 65, 0.18)`), preserving dark-mode contrast without obscuring character glyphs.