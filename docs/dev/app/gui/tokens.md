# GUI tokens, type, themes, prefs

## Brand overlay (locked)

Paint chrome, buttons, accents, cards with this overlay. Keep YAML token names as gpui-kit Theme keys.

Hex stacks live only in `gui/theme/tux.rs` (and community palettes). `paint()` copies them onto kit `ThemeColor`; chrome reads `cx.theme()`. `Brand` keeps only leftover roles kit does not have: `wash`, `on_selected`, ProtonDB platinum/gold/silver/bronze, `mauve`. `overlay_ink` takes a packed hex: art sample, else `hsla_hex(Brand.wash)`.

Default palette is **TuxGT Dark**. Dark **card** sits above **pane** (sidebar/titlebar). **Off switches use the outline token for a visible track; on switches use primary; the thumb uses foreground.** **selected** is the nav/outline lift. Body on cards uses **fg**; body on selected nav, outline actions, and other darker fills uses **on_selected**. Text on a photo or a coloured well picks light or dark ink from that surface (`overlay_ink`): hero title + mods line from the wash **art** (top band; no art → **wash** token); chips use the semantic colour as the well and `overlay_ink(well)` as the text. `pill()` normalizes the well by mode — dark mode pulls wells darker, light mode lighter, hue and saturation kept — so dark pills pair light ink and light pills dark ink; gray wells are for deemphasis only. CTAs keep their own fills.

Corners: controls and cards **8px** (2px on micro-badges and mono chips). Window frame **10px** when windowed (0 tiled/maximized). Depth = elevation (drop shadow + top highlight derived from the card fill) plus inner row hairlines. No outer card hairline. No last-row separator. No glass, no window-level transparency except CSD corner pixels.

### TuxGT Dark (default)

| Token | Hex | Use |
|---|---|---|
| **bg** | `#08090b` | Window, page |
| **pane** | `#0b0c0f` | Titlebar, sidebar |
| **card** | `#15181d` | Section cards |
| **selected** | `#2a3038` | Selected nav, outline-button fill |
| **inset** | `#050608` | Search well |
| **fg** | `#d0d5db` | Card values, idle nav, body |
| **on_selected** | `#d0d5db` | Text on selected / outline button |
| **muted** | `#8b939e` | Card keys, idle tabs |
| **hair** | `#2a2e36` | Row separators |
| **primary** | `#e0ae4a` | Tabs, focus, filled Play |
| **on_primary** | `#1a1206` | Text on primary fill |
| **accent** | `#6a8ea3` | Injection-ok, OptiScaler |
| **danger** | `#c45c68` | AWACY, destructive hover |
| **wash** | `#0e1014` | Hero fade |

### TuxGT Dark Alt (Blue)

Same stack. Icon ice-blue primary `#7ec8f0`, **on_primary** `#0a1820`, **wash** `#0c1822`.

### TuxGT Light

Dusty ice, not white.

| Token | Hex | Use |
|---|---|---|
| **bg** | `#c2cad2` | Window, page |
| **pane** | `#b6bec6` | Titlebar, sidebar, cards |
| **card** | `#b6bec6` | Section cards |
| **selected** | `#8d97a1` | Selected nav, outline-button fill |
| **inset** | `#aab3bb` | Search well |
| **fg** | `#262828` | Card values, idle nav |
| **on_selected** | `#2a323a` | Text on selected / outline button |
| **muted** | `#545a5c` | Card keys, idle tabs |
| **hair** | `#9aa3ab` | Row separators |
| **primary** | `#c4a45c` | Tabs, focus, filled Play |
| **on_primary** | `#1a1408` | Text on primary fill |
| **accent** | `#6a8ea3` | Injection-ok, OptiScaler |
| **danger** | `#b86870` | AWACY, destructive hover |
| **wash** | `#a8b8c4` | Hero fade |

### TuxGT Light Alt (Blue)

Same light stack. Primary `#5a9ec4`, **on_primary** `#0c1820`.

Tertiary (ReShade / addon) stays `#b48ead` / `#e3badb`. Warning stays `#e5c07b`. Popover is a step above **card**. YAML `primary` for chips/links may stay a lighter ice (`#84cfff` on dark); filled Play uses the palette **primary** above. Do not paint filled Play with `#84cfff`.

### ProtonDB tiers

| Tier | Hex |
|---|---|
| Platinum | `#b5c0d0` (well; ink from `overlay_ink`) |
| Gold | `#cfb53b` |
| Silver | `#a6a6a6` |
| Bronze | `#cd7f32` |
| Borked | `#ed5466` |
| Native | secondary mint (Library filter chip; not a ProtonDB tier) |

YAML `primary` `#84cfff` is chips/links. Filled primary buttons use `primary-container` `#3daee9`. Do not paint filled Play with `#84cfff`.

Theme keys from DESIGN.md: `surface`/`background` `#131315`, `surface-container-low` `#1b1b1d`, `surface-container-high` `#2a2a2c`, `on-surface` `#e4e2e4`, `on-surface-variant` `#bec8d1`, `outline` `#88929a`, `outline-variant` `#3e484f`, `primary` `#84cfff`, `primary-container` `#3daee9`, `secondary` `#4edcc0`, `tertiary` `#e3badb`, `error` `#ffb4ab`.

## Type

Bundle **Inter** (UI) and **JetBrains Mono** (paths, env names, hashes, ids, badges). OFL. No Google Fonts CDN.

Three scales (`font_scale` in `ui.toml`): Compact / Default / **Comfy** (stored id `large`). Token numbers are the **Default** ladder. Default is the product default. Compact/Comfy are 0.875 / 1.125 of that ladder. Do not invent a fourth ladder. Every scale must layout without overlap.

| Token | Family | Default px / line | Weight | Tracking |
|---|---|---|---|---|
| headline-lg | Inter | 18 / 24 | 600 | -0.01em |
| headline-md | Inter | 16 / 20 | 600 | -0.005em |
| headline-sm | Inter | 15 / 18 | 600 | 0 |
| body-lg | Inter | 15 / 20 | 400 | 0 |
| body-md | Inter | 13 / 18 | 400 | 0 |
| body-sm | Inter | 12 / 16 | 400 | 0 |
| label-lg | JetBrains Mono | 14 / 18 | 500 | 0.02em |
| label-md | JetBrains Mono | 12 / 16 | 500 | 0.01em |
| label-sm | JetBrains Mono | 11 / 14 | 500 | 0.04em |

Chrome 32 / 240 / 28 widths do not scale. Type tokens, control height, gaps, and content-sized rows do.

| Scale | Multiplier | headline-lg | body-md | label-sm | Control h | Product role |
|---|---|---|---|---|---|---|
| `compact` | 0.875 | 16 / 21 | 11 / 16 | 10 / 12 | 24 | dense |
| `default` | 1.0 | 18 / 24 | 13 / 18 | 11 / 14 | 28 | **shipped default** |
| `large` | 1.125 | 20 / 27 | 15 / 20 | 12 / 16 | 32 | comfy |

### Page CTA (Play)

`Play` / `Enable & Play` are the only controls above `control_h` (Game hero,
Library grid overlay) — `widgets::page_cta`. **h 32** at Default versus
`control_h` 28, extra horizontal pad (0 13), label `headline-sm` 15 / 18
semibold. Follows `font_scale` like `control_h`: **27 / 32 / 37** for
compact / default / large. No other primary inherits it.

## Theme presets

`ui.toml` `theme` id. Default **`tuxgt-dark`**. Switching recolors immediately.

| Id | Notes |
|---|---|
| `tuxgt-dark` | TuxGT Dark. Default. |
| `tuxgt-dark-blue` | TuxGT Dark Alt (Blue). Legacy `stitch` reads as this. |
| `tuxgt-light` | TuxGT Light. |
| `tuxgt-light-blue` | TuxGT Light Alt (Blue). |
| `catppuccin-mocha` | base `#1e1e2e`, mantle `#181825`, crust `#11111b`, text `#cdd6f4`, blue `#89b4fa`, teal `#94e2d5`, mauve `#cba6f7` |
| `nord` | polar `#2e3440` / `#3b4252`, snow `#eceff4`, frost `#88c0d0` |
| `dracula` | bg `#282a36`, fg `#f8f8f2`, purple `#bd93f9`, cyan `#8be9fd` |
| `gruvbox-dark` | bg `#282828`, fg `#ebdbb2` |
| `tokyo-night` | bg `#1a1b26`, fg `#c0caf5`, blue `#7aa2f7` |
| `breeze-dark` | view `#232629` / `#31363b`, plasma `#3daee9` |

Map each preset onto the same roles. ProtonDB tier hexes stay ProtonDB’s colors. Light presets use `ThemeMode::Light`.

## Persistence

File: `$PREFIX/config/ui.toml` (`config_dir()`). GUI-only.

| Key | Values | Default |
|---|---|---|
| `theme` | ids above | `tuxgt-dark` |
| `sidebar_collapsed` | `true` \| `false` | `false` |
| `font_scale` | `compact` \| `default` \| `large` | `default` |
| `type_ladder` | `1` | rebase stamp; missing/`0` rewrites old shipped `large` → `default` |
| `last_game` | game id triple | unset |
| `view` | `library` \| `library-list` \| `game` \| `settings` | `library` |
| `settings_tab` | `general` \| `core-plugins` \| `game-env` \| `mods` | unset → General |
| `settings_mods_tab` | `optiscaler` \| `reshade` \| `custom` | `optiscaler` |
| `debug_log` | `true` \| `false` | `false` (About switch; read at next start when `RUST_LOG`/`TUXGT_DEBUG` unset) |
Legacy `view = "prefix"` reads as `game` with General selected (prefix content lives on General). Missing or unknown `settings_tab` → General. Game inner tab is not a prefs key (that history is in-session). Back/forward history is not a key. No other keys without a product decision.

Code: `src/tuxgt/tuxgt-app/src/gui/prefs.rs`, `gui/theme/`.
