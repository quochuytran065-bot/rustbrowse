# RustBrowse Design System & Motion Specification

## 1. Visual Philosophy

RustBrowse combines two design inspirations into a zero-dependency, GPU-only desktop browser shell engineered for WebView2 on Windows 11:

- **open-design**: Structured semantic design tokens, crisp typographic hierarchy, high-contrast legibility, and clear surface elevation tiers.
- **animate-ui**: Tactile spring-like motion physics translated directly into pure CSS transitions and Web Animations API (WAAPI) using only compositor-friendly properties (`transform`, `opacity`). Zero React, zero Tailwind, zero runtime layout thrashing.

## 2. Theme Architecture

RustBrowse ships with two first-class themes toggled instantly in 1 click (`[data-theme="light"]` / `[data-theme="dark"]`, persisted in `localStorage` under `rustbrowse-theme`):

### Bright Glass (Default — `light`)
- **Shell Surface**: Translucent crisp frosted white-ice (`#eef3f8` base, `rgba(255, 255, 255, 0.84)` active toolbar)
- **Active Tab**: Pure elevated porcelain (`#ffffff`) with subtle soft shadow and hairline border
- **Dynamic Island Capsule**: Frosted pearl pill (`rgba(255, 255, 255, 0.92)`) with mint/sky focus halo and smooth pill expansion
- **Primary Ink**: Deep slate (`#0f172a`), secondary ink (`#475569`), muted ink (`#64748b`)
- **Accents**: Mint Teal (`#0ea5a4`), Sky Blue (`#2563eb`), Emerald (`#10b981`), Amber (`#d97706`), Rose (`#e11d48`)

### Midnight Dark (`dark`)
- **Shell Surface**: Deep obsidian-slate (`#0b0e14` base, `#131822` toolbar)
- **Active Tab**: Elevated graphite (`#1a202e`) with subtle top rim light
- **Dynamic Island Capsule**: Deep obsidian glass (`#0e121b`) with luminous cyan/blue focus ring
- **Primary Ink**: Crisp snow (`#f1f5f9`), secondary ink (`#94a3b8`), muted ink (`#64748b`)
- **Accents**: Mint Cyan (`#2dd4bf`), Electric Blue (`#3b82f6`), Emerald (`#10b981`), Amber (`#f59e0b`), Rose (`#f43f5e`)

## 3. Token Architecture (`ui/tokens.css`)

All surfaces, typography, spacing, radii, shadows, and motion curves are centralized in `ui/tokens.css`:

- **Surfaces**: `--rb-bg-shell`, `--rb-bg-toolbar`, `--rb-bg-tab`, `--rb-bg-tab-hover`, `--rb-bg-tab-active`, `--rb-bg-capsule`, `--rb-bg-overlay`, `--rb-bg-card`, `--rb-bg-elevated`
- **Borders**: `--rb-border-subtle`, `--rb-border-default`, `--rb-border-strong`, `--rb-border-accent`
- **Text**: `--rb-text-primary`, `--rb-text-secondary`, `--rb-text-muted`, `--rb-text-inverse`
- **Semantic Status**: `--rb-accent`, `--rb-accent-soft`, `--rb-success`, `--rb-warning`, `--rb-danger`, `--rb-info`
- **Typography**: `--rb-font-sans`, `--rb-font-mono`, scale `--rb-text-xs` (11px) to `--rb-text-2xl` (28px)
- **Radii**: `--rb-radius-xs` (4px), `--rb-radius-sm` (8px), `--rb-radius-md` (12px), `--rb-radius-lg` (16px), `--rb-radius-pill` (9999px)
- **Shadows**: `--rb-shadow-sm`, `--rb-shadow-md`, `--rb-shadow-lg`, `--rb-shadow-island`

## 4. Motion & Physics System

Every interactive transition adheres strictly to GPU-composited properties (`transform`, `opacity`) and never animates `width`, `height`, `top`, or `left` in hot loops:

| Token | Value | Use Case |
| --- | --- | --- |
| `--rb-ease-spring` | `cubic-bezier(0.22, 1, 0.36, 1)` | Dynamic Island morph, overlay pop-in, tab settle, button spring |
| `--rb-ease-snappy` | `cubic-bezier(0.2, 0.8, 0.2, 1)` | Hover micro-interactions, active press feedback |
| `--rb-dur-fast` | `140ms` | Icon press, hover highlights, badge ticks |
| `--rb-dur-base` | `180ms` | Tab highlight, island event pill enter/exit |
| `--rb-dur-slow` | `220ms` | Overlay entrance, theme crossfade |

### Accessibility
- `@media (prefers-reduced-motion: reduce)` immediately collapses all durations to `0.01ms` and disables keyframe animations while preserving state clarity.

## 5. Layout & Shell Ergonomics

- **Fixed Topbar Height**: `74px` (`36px` Unlimited Tab Strip + `38px` Navigation & Dynamic Island Toolbar).
- **Unlimited Tab Strip**: Flex-shrinking horizontal scrollable track (`min-width: 72px; max-width: 210px`) with a permanently pinned `+` New Tab button anchored on the right edge that never scrolls out of view even at 50+ tabs.
- **iOS Dynamic Island Capsule**: Centered pill in the toolbar showing a clean security status dot, compact domain readout or focused address input, plus live morphing event chips (ad blocked pulse, video detected badge, audio mini-bar, RAM clean feedback, crash recovery, update progress).
- **Floating Overlay Window (`ui/overlay.html`)**: Rendered in a borderless, transparent, always-on-top child WebView positioned below the triggering button so popovers (`ram`, `shield`, `videos`, `menu`) are never clipped by the 74px topbar boundary.
- **Rounded Viewport Frame**: Clean bottom hairline border and subtle inner corner framing connecting the topbar seamlessly into the page viewport.
