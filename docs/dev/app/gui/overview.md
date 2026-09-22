# GUI — native gpui-kit shell

GPUI-native chrome. Token source: `docs/dev/app/gui/tokens.md`. Visual reference (read-only): `external/Tux GT - Stitch/` for layout ideas only — not the shipped radius, palette, or card chrome. Stitch labels **Mod Manager / Game Inspector / Prefix Tools** are rejected.

Product chrome: **Library** and **Settings** are global. **Game** (tabs: General | Mods | Environment) is per-game. Left game list is always visible (collapses to an icon rail). Client-side decorations: the in-app titlebar **is** the window header (no KWin/SSD bar). Back/forward history; Settings and Library are jumps, not a gear toggle. Last operation is a **toast** at TopRight (below the titlebar), not a sticky overlay and not over the hero Play cluster or Tools. No bottom status bar.

Product rules: `docs/dev/app/core/`. Core wins on conflict. As-built: `docs/agent/MAP.md`.

## Strings

Every user-visible GUI string resolves through the Fluent catalog
(`src/tuxgt/tuxgt-core/l10n/en-US/cli.ftl`, `gui-*` keys) — static chrome and
live/data-driven text alike (labels, section titles, notes, chips/pills,
status/toast text, empty states, picker prompts). Templates keep their
placeholders with the existing `{ $arg }` + `Strings::get_args` mechanism;
the bundle disables bidi isolating so substituted text stays byte-identical
to the `format!` it replaces. New ids must be registered in the `GUI_IDS`
allowlist (`src/tuxgt/tuxgt-core/src/lib.rs`) so a missing key fails
`fluent_gui_catalog_resolves` instead of echoing the id to the user.
Non-visible literals stay literals: element ids, icon names, db keys/enum
values, URLs/paths, font names, tracing messages, CLI/tool names.

Tag-like **display** (chips, pills, filter menus, titlebar manager, Settings
inventory names) uses Fluent labels, not the stored id. Filter/sqlx/CLI/recipe
ids stay lowercase. Unknown id → show the stored string (do not Title Case it).
One GUI helper maps id → label (`widgets`); callers do not hardcode.

`gui-detect-val-*` **are** these labels (same keys). Manager labels already
exist as `plugin-*-label`. Add Fluent only for kinds that have no key yet:
`gui-store-*`, `gui-modtype-*`, `gui-arch-*`, `gui-adapter-*` (adapters and
plans), and `gui-tier-*` (ProtonDB; tier `native` reuses
`gui-detect-val-native`).

| Kind | Id | Label |
|---|---|---|
| Manager | `steam` `heroic` `manual` | Steam, Heroic, Manual (`plugin-*-label`) |
| Store | `gog` `epic` `amazon` `standalone` | GOG, Epic, Amazon, Standalone |
| Runner | `native` `proton` `wine` | Native, Proton, Wine |
| API | `dx9` `dx10` `dx11` `dx12` `vulkan` `opengl` | DX9, DX10, DX11, DX12, Vulkan, OpenGL |
| Bitness | `32` `64` | 32-bit, 64-bit |
| Prefix arch | `win32` `win64` | Win32, Win64 |
| Engine | `unreal` `unity` `re_engine` `creation` `blackspace` `mo2` | Unreal, Unity, RE Engine, Creation, Blackspace, MO2 |
| ProtonDB tier | `platinum` `gold` `silver` `bronze` `borked` `native` | Platinum, Gold, Silver, Bronze, Borked, Native |
| ModType | `reshade` `reshade_addon` `optiscaler` `custom` `effect` `texture` | ReShade, ReShade addon, OptiScaler, Custom Mod, Effect, Texture |
| Adapter / plan | `preload` `install` `proton_env` | Preload, Install, ProtonEnv |

ProtonDB pill stays `ProtonDB { $tier }` with the **label** in `$tier`. AWACY
status words stay mixed-case from the dataset (`Denied`); provider names stay
as stored. Not tags (leave as stored): game id triples, recipe ids, dest
filenames / `slot dxgi.dll`, Proton **build** strings, env var names, CLI
output. Chip **colors** still key on the id for ProtonDB / API / AWACY. Manager chips use muted ink (no Steam/Heroic/Manual colour).

## Layout chrome

4px baseline grid. **8px** corner radius on controls/cards (2px on micro-badges and mono chips). Window frame **10px** when windowed (0 tiled/maximized), with all four corners clipped. Inner row hairlines (not after the last row). No outer card hairline. No heavy blur, no glass, no window-level transparency except CSD corner pixels. Depth = elevation (drop shadow + top highlight derived from the card fill).

| Slot | Size | Notes |
|---|---|---|
| Titlebar | kit 34px | **Client** decorations. Left LTR: icon, **TuxGT**, sidebar toggle, Back, Forward, then context title. Right LTR: **Bell**, 1px hairline, custom min/max/close (34px cells, muted-circle hover on all buttons, close included — never `danger`) when Client, none when Server. Toggle/back/forward/bell are the same circle buttons (disabled back/fwd idle muted; bell pins its circle while the sidecar is open). The sidebar toggle is icon-only before Back: `panel-left-close` when expanded and `panel-left-open` when collapsed; its icon changes only, with no selected/on background. Windowed corners **10px** (0 when tiled/maximized). **No** titlebar Library, Settings, or Rescan controls. **No** 4-tab segmented nav. **No** app-version chip. **No** host Wine/Proton chips. **No person/avatar.** |
| Sidebar | 240px expanded, 48px collapsed | Every page. Owned `sb-*` rail (not the kit `Sidebar` — its scroll container caused scrollbar reveal, bleed, dual-scroll). Collapse control: icon-only in the titlebar before Back; persist `sidebar_collapsed`. Expanded: `sb-nav` section (Library/Settings rows, active row `sidebar_accent` + 2px primary left edge) with bottom hairline, `sb-search` section with bottom hairline, then the virtual game list — sections pinned, only the list scrolls, never an outer scrollbar. Collapsed: Settings + games as 32px wells (art or initials; **no text**, no names); tooltips carry the full `display_name`. Icon source: `gui.sidebar-icons`. Expanded game rows show the name plus optional `{n} mods`; no manager, clean, or pristine/modified dot. Rows **content-sized**. **No** manager chip row. **No** prefix/index footer. |
| Main | fluid, min ~560px | 14px inset from the rail and under page tabs. |
| Statusbar | none | Host identity lives on Settings → General → **Host**. Toasts: TopRight, top margin 34px + 16px. |

Stitch’s 300px right inspector is **not** a third dock.

### Window

Start from `TitleBar::window_options()` (drag / double-click zoom). Set
`window_decorations = Some(WindowDecorations::Client)`. Taskbar/pager name stays
`TuxGT`. Kit `TitleBar` paints min/max/close **only when** decorations are
actually Client — do not draw a second set. If the compositor refuses Client,
stop; do not ship double close buttons.

Visible frame corner radius **10px** on all four corners when not tiled/maximized; **0** when tiled or maximized. `Root` runs `bordered(false)`; the CSD frame is owned by the app (`gui/frame.rs`, adapted from kit 0.6.4 `window_border.rs`): same shadow/resize/inset geometry, transparent 1px stroke. Corner-touching containers carry the radius (titlebar top, sidebar bottom-left, main bottom-right). Keep CSD corner pixels transparent where needed. Upstream still clips children to rectangles, so rounding paints our own backgrounds only. Window and sidebar stay **opaque**; do not punch the desktop through chrome.


### Titlebar

Always `flex_shrink_0`. Height is kit `TitleBar` (34px). Width does not follow
font scale. Drag: empty titlebar + context title move the window (kit). Buttons
do not. Double-click still zooms.

Left, LTR: app icon (existing 16–20px) · app name (`gui-title`) · **sidebar toggle** (`panel-left-close` when expanded, `panel-left-open` when collapsed) · **Back** (disabled at start of history) · **Forward** (disabled when no forward entry) · then the **context title** (`flex_1` `min_w_0` truncate), not a second app name.

| Page | Title |
|---|---|
| Library | `Game Library` |
| Settings, first tab (General) | `Settings` |
| Settings, other tab | `Settings / {tab}` — short: `Core Plugins`, `Game Env`, `Mods` |
| Game, first tab (General) | `{Manager} / {display name}` |
| Game, other tab | `{Manager} / {display name} / {tab}` — short: `Mods`, `Env` |

Manager in the title uses the label map (Steam / Heroic / Manual), not the raw
id. Separator is space-slash-space (` / `). First tab is General (Game) and
General (Settings). Inner TabBar clicks push a history entry, so
back/forward cross tabs, not just top-level pages.

Right, LTR: **Bell** · 1px hairline ~12px tall, muted, not a `\|` glyph · custom min/max/close: 34px cells (min/max only when the compositor supports them, restore swaps in when maximized), 24px muted-circle hover on every button including close. The product bar owns drag, double-click zoom, and the right-click window menu; kit `TitleBar` is dropped (it hardcodes Close hover `danger`). If decorations are Server, no buttons are painted.

Library and Settings remain sidebar jumps. Empty-library **Configure paths** still opens Settings. Mouse back/forward (side buttons) match the titlebar Back/Forward.

### History

In-session only. Not `ui.toml`. A **place** is Library, Game `{id, tab}`, or
Settings `{tab}`. Push when the **page or tab** changes through sidebar/card/tab navigation or empty-library Configure paths, and the new place ≠ current. New push **clears** the forward stack. Cap 20.
Do **not** push: filters, search, Play, theme, or the titlebar sidebar toggle.


Back/forward restore the place (game id + tab, settings tab, library
grid/list). They do not push. Restoring a place updates live nav/selection/tab
and persists `last_game` / `view` for that place (so restart matches the
current page). The stack itself is not persisted. Game inner tab is not a
prefs key: restart of a game opens **General**. Legacy `view = "prefix"`
opens Game → General.

### Host identity (Settings)

No status bar. The same probes paint Settings → General → **Host**. Honest
unknowns, **identity not live meters**. No extra timers. Do not poll usage. Do
not use `lspci`, `hostnamectl`, or DBus only for this card.

| Field | Source | Show |
|---|---|---|
| OS · kernel | `/etc/os-release` `PRETTY_NAME` + `/proc/sys/kernel/osrelease` | `ExampleOS · Linux 6.8.0` |
| DE · compositor · session | DE as below; compositor env / desktop-token map (`KDE`→KWin); `XDG_SESSION_TYPE` | `KDE · KWin · Wayland` |
| CPU | `/proc/cpuinfo` first `model name`; else `Hardware` (ARM) | strip trailing ` CPU` / ` Processor` / ` N-Core(s)` |
| RAM | `/proc/meminfo` `MemTotal` | whole GiB, floor of kB/1048576 (`32 GiB`) |
| GPU | existing `gpu_name` | Fastfetch rule: prepend vendor only if the model does not already start with it. Keep GeForce. |

`detect.gpu` stays for Env Advance. `gui.statusbar-runtime` stays dropped.

### Tray (`gui.tray`)

One StatusNotifierItem. The Hide to Tray card offers On Game Launch
(default on), On Minimize (default on), and On Close (default off);
with no SNI host the switches stay disabled and Close stays a real
close. A successful GUI or tray-menu Play hides the open window and
leaves it hidden until the user re-shows it; a failed Play stays
visible with the error. While hidden there is nothing to hide.
Primary activation toggles Show/Hide, never quits; the menu also quits.

The menu is state-aware sections, rebuilt on every open: one of Show /
Hide (never both), then Show Library and Show Settings page jumps, then
up to 5 recent games by `last_played` (hidden rows excluded, omitted
entirely when empty — no placeholder), then Quit last. A recent click
plays headless with GUI Play semantics and no arming; a success hides
the open window when the toggle is on, failures log and surface in
the status line.

Hide hands the session to the tray stub (`tuxgt tray --handoff`) and the
GUI exits, so a hidden session is one small SNI client instead of the
whole window/GPU/font working set. The stub re-reads prefs and recents
(nothing is handed off); any Show respawns the GUI to the persisted
page and the stub leaves, and a second `tuxgt gui` while hidden is
answered the same way.
A Hide is refused while a transfer, mutation, or parked confirm is in
flight (busy line); Quit always terminates. A Play failure while hidden
waits in the prefix for the next window's status line. The catalog
update poll never runs while hidden.

Control height follows `font_scale` (24 / 28 / 32). Titlebar/sidebar **widths**
do not follow font scale (collapsed rail is 48px). Gaps, min heights, and card
caption boxes **do**. Shipped default scale is **Default**; layout must not
overlap at that scale.

Scroll and flex rules: `landmines.md`.

### Tooltips (`gui.tooltip`)

`.tooltip(...)` strings come from `Strings::get`; overlay show/hide, placement,
and dismissal live in the gpui-kit managed tooltip (upstream crate), not the
app. A Back or Forward click leaving the previous tooltip on screen is not
reproducible.

## Files in this folder

| File | Job |
|---|---|
| `tokens.md` | Color, type, themes, `ui.toml` |
| `library.md` | Library page |
| `game.md` | Game hero + Mods/Env/Launch/Prefix/Info |
| `prefix.md` | Prefix tab |
| `settings.md` | App settings (General \| Core Plugins \| Game Env \| Mods inner tabs) |
| `landmines.md` | gpui-kit traps; screenshots; ops |

## Recipes (all pages)

Heights at Default scale. Compact/Comfy multiply type/control as in `tokens.md`.

One role, one widget, one place (`gui/widgets/` helpers: `value_btn`,
`destroy_btn`, `open_btn`, `section_header`):

| Role | Widget | Place |
|---|---|---|
| Enable object / preference | Toggle | Property row: last, right. Object card: first, left of title. |
| Include in a set / keep / inherit-vs-set | Checkbox | First, left, in the 24px column; the selectable row label is clickable and toggles the checkbox. |
| Set a value | Button — Value | Last, right. Secondary + trailing `ChevronDown`. |
| Row action | Button — Action | Right cluster after a spacer. Secondary. |
| Destructive | Trash icon | Last in the cluster. Danger. Tooltip is the word (Remove / Uninstall). |
| Disclosure (files, effects, advance) | Accordion / chevron | Not in the action cluster. |
| Open external | `type_icon` + label + `ExternalLink` | Type: Folder (dir), File (file/exe), Link (URL). Trailing icon always `ExternalLink`. |
| Page CTA | Play / Enable & Play | Hero left column, bottom-left (`game.md`). |
| Section CTA | Install, Add, Save, Rescan libraries | Section header only. Form Save/Cancel stay in that panel's footer. |
| Exclusive two-way mode | `ToggleGroup.outline().segmented()` | Load \| Include. Not a Switch. |

List types:

- **Property row** — 0–1 trailing control. Identity left, control right,
  `justify_between`, hairline. Knob rows already match (checkbox + value).
- **Selectable list** — checkbox 24px + title + optional disclosure/link. Footer
  owns Add/Install. Clicking the row toggles the checkbox; nested inputs and
  actions stop propagation.
- **Object card** — 2+ actions. Accent stripe, toggle + name left, actions right,
  extra lines / accordion below.
- **Toolbar / section header** — title left, actions right. If the action cluster
  would wrap, replace it with **⋯** (`Ellipsis`) whose menu is those actions.
- **Hero** — Play cluster bottom-left, after title/chips/status.
- **Choice group** — Launch Mode stays stacked selected-primary.

Icon rules:

- **No leading Plus** on labeled Add/Install buttons. Plus appears only as an
  icon inside the **⋯** menu.
- A wrapping action cluster collapses to **⋯** (`Ellipsis`) — never a second
  button row.
- Open-external controls are `type_icon` + label + trailing `ExternalLink`; the
  destructive trash carries its word as the tooltip, not on the control.
- Select All copy is **Select Visible** (`gui-action-select-visible`): the rows
  the filter left visible, not the whole set.

- **Primary button** — bg `#3daee9`, fg `#ffffff`, no border, h 28, pad 0 10, Inter 13/18 600. Active `#2a99d3`. Play uses this.
- **Secondary** — bg `#2a2a2e`, border 1px `#38383e`. Hover bg `#333338`.
- **Destructive** — danger trash icon button, last in the action cluster: `#ed5466` icon, transparent bg, hover solid. The word (`Remove` / `Uninstall`) is the tooltip — never a worded or `Close`-glyph control.
- **Badge / chip** — h 18, radius 2px, JetBrains Mono label-sm.
- **Injection dot** — 6px. Mint `#2bc4a9` modified; amber `#e5c07b` mismatch; `#5a5a64` pristine; danger `#ed5466` AWACY.
- **Game tabs** — underline TabBar on the Game page (General | Mods | Environment). Settings uses the same TabBar for General | Core Plugins | Game Env | Mods. No titlebar segmented nav.
- **Two-col** — `h_flex().items_start().gap_3()` of two `v_flex().flex_1().min_w(px(280.)).min_w_0()`. Wrap below ~560px.
- **Labeled row** — `justify_between`: label (+ optional help) left, the control last at the far right; the label column is capped and truncates.
- **Mod card** — 3px left accent: ReShade / `reshade_addon` / `effect` / `texture` `#b48ead`; OptiScaler `#2bc4a9`; `custom` `#3daee9`.
- **Toast** — the app's own store (`gui/notice.rs`), not the kit
  `NotificationList`: three lanes feed one TopRight stack (top margin 50 =
  titlebar 34 + 16). **Live** (install progress): overlay unless snoozed, always
  in the sidecar, uncapped; X snoozes that surface, never cancels. **Activity**
  (every `status` / `pending_note` flush): overlay until autohide — info/ok 5s,
  warn/err until X — then the last 10 in the sidecar; sidecar X deletes one row,
  **Clear all** empties Activity. **Attention** (from the 2h catalog
  poll): sidecar only, never an overlay, uncapped and deduped by key
  (`catalog:<id>` → `{label} — update available` jumps to Settings Mods;
  `game:<id>` → `{display} — {n} updates` jumps to that game's Mods);
  X is a session-dismiss, dropped when UpToDate/gone. A successful
  per-game Update recounts that game immediately and drops `game:<id>`
  when nothing there is still stale, without waiting for the next poll.
  Unknown never cards. An `app:<tag>` card is part of `app.self-update` and is not in this tree.
  The card X is at kit `small` (24px), ghost, hover-only. The **Bell** (left of
  the titlebar rule) opens the sidecar — no pane, cards + ghost Clear all — and is
  ghost, primary `#3daee9` while Live is visible, warning `#e5c07b` while
  Attention is undismissed; no count badge, no history push. A click outside,
  the bell, or Escape closes it. The panel occludes the outside catcher so X
  and Clear all receive the click; wheel over the list does not scroll the
  page. Overlay and sidecar never paint together;
  process death wipes the store (no history persistence).
- Placeholder controls stay **visible**, 70% opacity, not-wired tooltip. **No fixture rows.**

## App icon

Rounded mark from `external/tuxgt-app-icons/` (read-only source). Vendored: `src/tuxgt/tuxgt-app/assets/icons/hicolor/<16..1024>/apps/tuxgt.png` (9 sizes) + baked 64px `assets/icon.png` fallback. Titlebar reads `share/icons/hicolor/64x64/apps/tuxgt.png` (legacy `share/tuxgt/icon.png`, then baked). Desktop is themed `Icon=tuxgt`; `tuxgt install` writes all 9 to `~/.local/share/icons/` and tracks them as one `icons n/9` entry.

## Forbidden

Lutris / Bottles / Faugus as first-party; accounts/avatar/telemetry/ads; Qt/Tauri/WebView/iced in the product binary; GNOME-specific chrome; Deck big-picture; play-triangle-as-logo; fake version `v1.4.2`; “KWallet Linked”; Proton version manager UI; copying Stitch HTML/CSS into `src/tuxgt/`.

## Skipped / not achievable

- Per-knob text inputs on Env: gpui-kit `Input` needs a live `InputState`; 50+ entities rejected. Booleans (single value, or a 0/1 pair with the value helps on the switch tooltip) are switches; other listed knobs are dropdowns; custom env add is one row.
- Window icon via compositor: gpui 0.6 has no `set_window_icon` on this platform. Baked `assets/icon.png` is the last-resort fallback after the themed path then legacy `share/tuxgt/icon.png`.
- Isolated Wayland client screenshots: spectacle `-a` did not isolate; use monitor capture.

## Live verify

Live verify runs in a private GUI lane (`src/tuxgt/tools/gui-session`, procedures in
`landmines.md` § Ops): headless sway, grim captures, virtual-pointer/keyboard
input (`src/tuxgt/tools/wlinput` daemon + `wtype`), scratch prefix per lane — no window
on the operator's desktop and no input path to their session, several lanes at
once. The operator session is only for Wayland-surface checks (CSD frame,
tiling/maximize, statusbar host line, keyring Secret Service).

## CLI

`tuxgt mods export <id> --out <path> [--files]` exports a user Mod recipe (recipe-only TOML, or recipe + kept payload files as a zip); see `docs/dev/app/plugin/instances.md` (Export).
