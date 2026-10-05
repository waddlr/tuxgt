# Game page

## Hero (`gui.game-hero`)

Full-bleed wash in the main pane with a guaranteed minimum height; no bordered hero card; no second cover thumb.
The overlay content is bottom-anchored — Play row → status row → title last — over a true fade just above the TabBar (no gap between banner and tabs).

- Wash: **wide art only**. Prefer Steam `library_hero` (then `library_header` /
  `header`) as `header_path`. Width-fill crop-fit. **Never** a
  portrait cover as the banner (`ObjectFit::Cover` on 600×900 is a mid-body
  crop). No wide file → no empty box; title + CTAs sit on **bg**. Then `$PREFIX/config/cache/art/<id>/hero`
  from a remote `header_path` URL or SteamGridDB `hero_url` (not grids)
  (`gui.metadata-fetch`). No heavy blur.
- Overlay, left column (no right column), top to bottom: **Play** row,
  chips + **Mods** / **Knobs** pill row, **display name** last (`headline_lg` + 2,
  20 at Default, still following `font_scale`).
  Title paints theme **foreground**; the pills carry solid wells. A true fade
  (transparent at button half-height, darkest-translucent at the title) sits
  behind the stack.
  No art → palette **wash**.
  Chips use their semantic colour as the well (`overlay_ink` on that well).
  CTAs keep their own fills. ProtonDB tier chip **only**
  when the cached/fetched tier is not `none` / missing — opens
  `https://www.protondb.com/app/<appid>` when an appid exists. AWACY **only**
  when matched and not `none` (danger token). No manager pill (titlebar already
  `{Manager} / {name}`).
- Play cluster: one row, **bottom-left**, shrink-to-label (not full width).
  **Enable & Play** + **Play** side by side when both paint, else **Play** alone.
  **Enable & Play** full **primary**; **Play** **primary** when hooked
  (handled/applied), gray **secondary** when vanilla. The large page CTA
  (`tokens.md`: h 32 at Default; no header
  Apply/Restore; Launch Mode on General performs apply/restore).
- **Mods** and **Knobs** status, not Hidden.

  - Mods = enabled instances.
  - Knobs = enabled per-game env knobs that are set + custom KEY=VALUE +
    wrappers + detection overrides.
  - Both 0 → one muted **Unmodified**.
  - Else `{n} mods` or muted **No Mods**, plus `{n} knobs` or muted **No Knobs**.
  - Active counts: mint. Unmodified / No *: muted body.

**No TARGET chip.** Drop from the hero: manager pill, Hidden, id triple, exe,
proton, prefix, API, bitness, Copy launch.

Hero is identity + Play + modification status. Not a second detector.

Store rows: **Launch Mode** radio on General is source of truth (`launch.needs`).
Vanilla + needed channel (preload / env / argv wrappers / install) → primary **Enable &
Play** (Apply when Hook is illegal: argv wrappers, or any enabled install instance;
else Hook-if-GE-else-Apply). Already
Hooked or Applied: Play only. CLI `tuxgt launch <id>` asks Enable & Play when that
button would paint (`--yes` arms and plays; `--vanilla` plays unmodded; a declined
Enable & Play then asks Play vanilla). **Not hooked** is always selectable — it pauses
injection; when needs return, Enable & Play reappears (re-arm is another
click). The **Hidden** switch lives on General → About
(“Hide from TuxGT library”); it still writes the per-game override (wins over
the detected flag and survives rescans).

Toasts: the app's own store (`gui/notice.rs`), not the kit
`NotificationList` — **TopRight**, top margin = titlebar 34px + 16px, so
they do not cover the hero Play or Tools.

Play toast copy: the game **display name** plus **modded** / **unmodded**. No
armed instance list. Errors stay errors.

## Tabs

**General** | **Mods** | **Environment** — underline TabBar, icon + rendered
label each (General / Mods / Environment). Default **General**. Do not default
to Mods or Environment. Same kit recipe (`prefix` icon + `label`, with
`aria_label`; ExtraIcons only for catalog icons already in use). Do not
embed `AllAssets`. Never the kit icon slot: its 27.5px box shifts the
underline left of the visible content.

Titlebar: first tab (General) is `{Manager} / {display name}`; other tabs
append ` / Mods` or ` / Env`. Restart of a game opens General. Legacy
`view = "prefix"` opens General.

General is two rows then Advanced. Mods is instances. Environment is knobs +
custom env.

Play is `docs/dev/app/core/launch.md`: Steam/Heroic dispatch to the store client.
Hook mode → GE/Cachy injects via protonfixes (no Apply). Applied mode →
trampoline injects (self-arms from argv). Not hooked → vanilla.
Manual rows: Play injects; no store Apply, out of the radio. If spec build
fails, show the error; do not fake a launch.
If a required host file under protonfixes `localfixes` has drifted, Play and
Apply still run and add a warning toast pointing at Settings → General.

Lazy fetch, with hero Knobs needing counts on every tab: game select
loads detection + launch state (wrappers + cached launch config) + metadata
(`game_show`, not on paint); Env loads the env rows (knobs + custom), every
other tab loads knob counts only. General also loads extras + AppID. Mods
fetches mod extra. Env refreshes env.

## General

Row 1: **About (60%) | right stack (40%)**: **Launch Mode** (store rows
only), then stacked **Extra wrappers** + **Tools**. No second row. Then
**Advanced**.

No page-level diagnostics banner. Do not paint: effective launch preview,
prefix tree, doctor, snapshots, a separate Identity or Paths card.

### About

One pane, this order. Every key-value row is key left, value right,
hairline below, regular ink never muted; long values truncate with a row
tooltip carrying the full value.

1. IDs. Steam: key `Steam ID`, value a button with the appid that opens the
   Steam store (`https://store.steampowered.com/app/{appid}`), tooltip = URL.
   Non-Steam: key `{Manager} ID`, value the game segment as plain text; an
   existing Steam AppID overlay appends a `Steam ID` row as that same store
   button. Heroic/manual without an overlay keep the AppID editor directly
   under the manager-ID row: full-width input with inline Save | Search
   (Search uses the input, falling back to the game title when blank, plus
   one edition-suffix retry) so ProtonDB can resolve.
2. ProtonDB rating: tier label; a link opening the ProtonDB page when an
   appid exists, honest `none` text otherwise. Opening the game fetches
   summaries via `game_show` (not paint-path HTTP).
3. Runner under the Detection proton title. Omit when native or unset.
4. No build/version row: Steam exposes only a numeric buildid and Heroic's
   merged build channel is usually a build id too - neither is the game
   version users recognize. The Detection tab keeps its build entry.
5. Launch config as read-only rows (present values only): Launch Options,
   Launch Wrapper as single lines; Launch Env as a sorted `KEY=VALUE` list
   (`GameLaunchConfig::env_lines`, raw fallback when not JSON) with three
   lines visible and its own scroll. Nothing stored paints one
   `Launch Config | (none)` row. Steam launch options are not in our row:
   `steam_launch_options` reads them live from the client `localconfig.vdf`
   at selection load.
6. **EXE**, **Install Path**, **Prefix Path** — secondary buttons. Tooltip =
   full path. Click `xdg-open`s the location (file → parent dir). Never exec
   the PE. Disabled when the path is missing.
7. **Copy TuxGT ID** in the header (copies `manager:store:game`, tooltip = the
   id).
8. Hide from TuxGT library switch (per-game override only; does not change
   the store).

### Launch Mode

Store rows only. Copy:

- Hook protonfixes — “Inject without changing Steam/Heroic settings. Needs
  Proton-GE or Proton-CachyOS.”
- Update Launch Options — “Puts TuxGT on the store Play button. Restart
  Steam/Heroic once.”
- Not hooked — “Plain store Play.”

Disabled Hook reason stays one line: Proton-GE / Proton-CachyOS, or argv
wrappers need Update Launch Options. Argv wrappers on → Apply only. Preload or
env (no argv) → Hook (if GE/Cachy) and Apply. Install without preload disables
Hook and requires Update Launch Options; Not hooked stays enabled. Nothing TuxGT: Hook and Apply hidden, Not hooked
is the default. Illegal click is a no-op.
**Not hooked is always painted and enabled**: it pauses injection and never
touches mods, knobs, or wrappers. Enabling gamescope / GameMode /
MangoHud-as-argv while Hook is armed auto-switches to Update Launch Options.
Needs dropping to none while Hook/Apply is armed auto-restores: hook-only
clears the handle even while the store client runs; Apply restore still
defers until that client is gone. The radio still paints the armed arm until
core has disarmed, so a deferred Apply is not an unselected Not hooked with
Hook/Apply hidden. Like a manual Restore the store client still needs its
restart for that to take (a running Heroic gets the same restart notice).

### Extra wrappers

GameMode, gamescope, MangoHud. Label = product name. Help = tooltip only.
One muted line when Hook is selected and wrappers are off: gamescope and
GameMode need Update Launch Options. Manual rows: wrappers still apply
on owned Play.

Adapter **Preload | Install** (`gui.adapter-choice`): code kept, **not
painted**. Switching Install does not copy dests for existing preload
instances. Default remains preload. Wrappers are independent of file
placement.

### Tools

`docs/dev/app/gui/prefix.md`. winecfg, regedit, explorer, winetricks. Native:
muted no-prefix note (install path is About’s Install Path button).

### Advanced (collapsed)

Kit Accordion. Body: Extra launcher exes (**Add** in the section header, no Plus;
row = path left, trash right), then Detection (stacked rows + Force redetect;
one **Override** `value_btn` per field — Unset + values, or
Browse… / Unset for exe/prefix; free-text Edit lives inside that menu).

## Detection rows

One block per field, not a single cramped `labeled_row`:

- Title = Fluent field label (`gui-detect-field-*`); paths stay path-like.
- One source pill: Detected / Override / Store / Unset.
- Effective value on the next line, `min_w_0` truncate, full path in tooltip.
- One **Override** `value_btn`: Unset + enum values, or for `exe`/`prefix`:
  Browse… / Unset. No adjacent Browse+Edit+Clear.
- Do not list every source value as subtext.

`extra_apis` / free text: Edit lives inside the Override dropdown (inline
input below the row). Redetect stays at the bottom of Advanced.

## Mods

Instances of Mods on this game. Install picks a **Mod** from the catalog and
creates an Instance — register a Mod in Settings → Mods first. No per-game
file drop, no orphan `custom-{stem}`.

### Installed list (`gui.mods-install`)

Only **installed** Instances (`FileManifest` exists). Three sections —
**OptiScaler | ReShade | Custom** — by the `SettingsModsTab` membership and
order the Settings → Mods inner tabs use (pack kinds `reshade_addon` /
`effect` / `texture` ride ReShade). Section header is the tab label plus a
▸/▾ marker and is clickable — it collapses that section (default expanded);
a collapsed section paints its header only and is out of Uninstall-visible
scope. An empty section paints nothing; installed sections have **no** subheads.
Inside a section officials are **pinned first**, then
(`load_order`, instance).

Card: object card — enable switch + name left; the action cluster
(move up / move down + trash) right. No type pill
(the section names the type); its per-type help tooltip rides the name. No
Provides pill. Effects / Files previews are disclosures **inside** the
accordion, never card-header buttons. Graph line only for missing requires or
slot conflicts.

**Slot** (`gui.mod-slot`) — `value_btn` on the card's second line, slot-capable
installed cards only. The GUI paints section-major order and writes it back on
any reorder (`set_load_order` still receives exactly the game’s installed ids); the engine
order is the stored `load_order`, which a fresh install appends to
(install order) and which only a reorder rewrites. The move cluster
(to-top / up / down / to-bottom) moves one card inside its own section,
within its same-officialness run; a user card never passes the pinned
official above it (nor an official a user card).

Every contested dest marks both sides. The card header carries a
conflict icon — warning tint when the card loses any file, success
tint when it wins them all — whose tooltip names each contested dest
with the rivals on each side; each contested file row in the expanded
list carries the same marker, its tooltip splitting the other
providers into loses-to and wins-over. Markers are display-only:
reordering stays on the card, where every dest the card loses paints
a row with a Make-win button that moves this card past that group's
other members. Groups no reorder can win — a contender in another
section, or a group mixing official and user cards under the pin —
paint the row with a disabled button that says why.

Tab chrome: one header row above the list, even when empty — the installed
filter left, **Install** (primary, no Plus), **Force re-sync all** (outline +
hairline) and **Uninstall visible** (trash, tooltip) right; the cluster
collapses to **⋯** when it would wrap. While the picker is open this row hides
and the picker panel paints in its place. No Install/Prefix/TuxGT dir row
under the list (Prefix Open, Advanced TuxGT dir, About path Copy).

Empty: “No mods installed. Install from the catalog (Settings → Mods first if
the list is empty).” **No fixture rows.**

### Picker (`gui.mods-picker`)

Inline panel in place of the filter + actions row (that row hides while the
picker is open). Header: title + **Install** (disabled if none checked) ·
Cancel. Cancel closes with no writes. When open, the installed list hides.

Body: applicable Mods without an Instance (registered, enabled, `games`
globs; disabled Mods omitted), in the same three sections (OptiScaler |
ReShade | Custom) with the Settings Mods page’s subheads as visual breaks —
**Official**, **User mods**, and **User Packs** inside ReShade for the pack
kinds (officials first, like `pack_rows`). A section header is clickable and
collapses that section (default expanded); subheads never collapse; empty
sections and subheads paint nothing. Each row: clickable checkbox row + label (tooltip =
Mod id) + previews. Multi-select. No game-level file add.

Row previews (Settings user-Mod cards follow the same rule): **Effects ▸**
on effect Mods whose recipe carries an `EffectFiles` list (minted packs; names
have the recipe's own drop globs applied, first 10 shown with `+ N more` expanding to the full list), and
**Files ▸** only when there is something to list — the local payload walk once
the payload lands (unchecked add-form files excluded + built-in repo junk excluded, same as install), else the
addon archive name taken from the recipe source (manual-url basename, or a
wildcard-free github asset). A row with neither paints no preview button.

**Select Visible** (first row, aligned with the entry lines,
`gui-action-select-visible`) covers the expanded sections only, with the picker
filter applied, and appends in the order the picker paints (section order, then
that section’s subheads and rows) — the batch install order. Collapsed sections
are out of Select Visible scope, so a check/uncheck made before collapsing
survives either way.

Adapter Preload | Install stays unpainted (`gui.adapter-choice`).

Install: close the panel, then run the existing `install_mod_ui` **queue**.
Check order, except ids already in that batch that satisfy another selected
id's type or recipe requires run first. Unchecked requires are not
added. A picker Install while a spawn is in flight (or parked on a confirm
card) **appends** ids that are not already current/queued. Toasts stay.
`NeedConfirm` / `MissingRequires` still **stop** (`confirm_card` on the
tab, never silent `yes`). After Confirm, continue the queue. Error: stop the
queue, keep what already installed, toast. Do not keep the panel open during
download. `MissingRequires` parks **one** card listing every missing dep (type
requires then recipe requires, transitive over the catalog minus installed
manifests): one candidate paints as a `- Name` bullet, 2+ as one dep line with
a horizontal glyph radio (`●`/`○`, first pre-selected, click to switch). One
**Install required** queues the chosen deps topo-first, then the target, ahead
of the rest. An empty closure, or a dep with no candidate, falls back to the
single-miss card or the status line.

Empty picker: “All applicable mods are installed” (or none apply). Install
stays disabled.

### Install Live card (`gui.notifications`)

Every GUI `install_instance` — Install (picker or card), Update redownload, and
the requires-chain core runs for `with_requires` — opens **one** Live card per
`(game, instance)`: `Installing {label}…` (`gui-notice-installing`) with an
indeterminate bar while core is in a phase with no bytes to count (cache hit,
local copy, unpack, staging), then kit `Progress` 0–100 once `fetch_url` reports
a `Content-Length`. No length — or no report at all — keeps the bar
indeterminate: never a percent that never moves. Progress repaints at most once per 100ms
and per 1% step.

Success finishes the card as an **Activity** toast `Installed {label}`
(`gui-notice-installed`, Ok, 5s autohide); a failure finishes it as an **Err**
Activity that waits for X. `NeedConfirm` / `MissingRequires` finish the card with
**no** toast — the confirm card, or the status line, is the follow-up.
Overlay X snoozes that surface only (the install keeps running, confirm card unchanged);
the finish toast un-snoozes it as a normal Activity toast. No cancel from X. Art,
hero, host-install and scan are **not** Live.

### Update (`gui.mod-update`)

Available shows Update button only (no note) / Unknown short note + Update / UpToDate quiet. No CLI
text, no game id. Provenance and cache self-heal still runs before an Unknown note. The 2h catalog poll feeds
poll-fresh Available onto the card without a GitHub hit on tab open (the
poll already ran `check_catalog_update`); the per-card check still serves
Unknown and refreshes after Update. Unknown never comes from the poll.
Update does not confirm payload drift or per-game staged edits: the
redownload replaces depot bytes and staged touches are kept. Foreign
game-dir overwrites still stop on the confirm card. A successful Update
recounts that game and drops its Attention card when nothing there is
still stale.

### Files + env (`gui.mod-files`, `gui.stage-status`)

One kit Accordion per card, default closed, title = file count. Accordion
background is the **card** token (not kit dark `#0a0a0a`). Dest + env keep
rows: checkbox 24px · mapping `flex_1 min_w_0 truncate` · sync pill; `py_1` +
1px hairline between rows. An applicable `.dll` (not a `pfx:` prefix copy) also has a Load switch (`gui-mode-load`) left of the pill: on emits `LoadDLL`, off emits `IncludeFile`, and the row moves section. Non-dll rows have no switch. The choice is this game only and does not change keep, required, or slot claim. Required dests: checkbox on, disabled, tooltip.
Per-card Force re-sync sits **inside** the accordion footer (tab-level Force
re-sync all stays above the list). Both outline + hairline.

### Config edit (`gui.mod-config-edit`) — every allowlisted text config dest row carries an icon-only Edit (file-pen-line) left of the sync pill (enabled dests only). The Mods tab takes over for a full-page editor of the staged copy; Save flips the pill to user-modified. Opening externally closes the page. Navigating away closes the page too: clean buffers close silently, unsaved edits park a Discard confirm. Force re-sync still overwrites staged edits by design.

## Env
env var** (`DXVK_HUD`); a bundled knob joins every var it writes with ` · `.
Enable checkbox on the left (off = inherit). Inherit display: enabled game value, else unmanaged live, else enabled global. Per-game override of unmanaged stays. A single-value boolean switch off on a game
row is override-off: the checkbox stays on and launch writes empty `VAR=`
assignments so a global/unmanaged value does not apply. Knobs whose listed
values are exactly `0` and `1` are toggles too: on writes `1`, off writes
`0` (explicit, both pages), and the switch tooltip carries both value
helps. Help is a **tooltip on the
label**, not a second line: first line = full env var(s), then the help
(same ink as the key, not muted).
Hairline between rows. Main
groups always open; collapsed **Advance** holds other-GPU groups and
GE/Cachy-only knobs when this game’s proton is not CachyOS/GE; mesa stays in
main and mirrors into Advance only on NVIDIA hosts; DX12 sends DXVK to
Advance while other known APIs send VKD3D there; set/unmanaged stay in main.
A vendor group leaves main only when that vendor is absent from the host.

No client-session essay. Optional one line: “Applies on the next Play.”

Custom environment: **Add KEY=VALUE** sits in the section header (no Plus) and
opens the inline row; each entry is `KEY=VALUE` + trash; hairline between
entries; empty “No custom variables.”

## Copy / buttons

`widgets::copy_btn`: `.secondary().ctl()`, Copy icon + label. Use for game
id, AppID, exe, install dir, prefix path, launch preview. Open stays
secondary. Primary = Install, Play, Run doctor, Enable & Play — Play / Enable &
Play are the large page CTA (`widgets::page_cta`, `tokens.md`). Destructive =
Uninstall / Remove (trash, word as tooltip), redetect confirm. Outline+hairline
= Force re-sync.

## Copy that must not ship

No `gui-note-client-launch`, `gui-note-client-knobs`, `gui-note-mod-files`,
adapter-help as body copy, `gui-note-wrappers-store`, `gui-note-env-preview`,
`gui-note-prefix-diag`, `gui-note-detection`, extra-exes correlator essay as
body, `gui-note-steam-appid` as body (tooltip ok), doctor CLI string,
snapshots unwired note, per-card mod-type help paragraph, wrapper/knob help
as always-visible subtext, duplicate Copy launch, Mods dir row.

Code: `src/tuxgt/tuxgt-app/src/gui/game/`, `prefix.rs`.
