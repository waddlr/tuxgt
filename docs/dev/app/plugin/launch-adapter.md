## LaunchAdapter + prewire

Types may gain fields; not an external ABI.

Not in this surface:

| Item | Where |
|---|---|
| Heroic direct exec (`legendary`/`gogdl`/umu owned by tuxgt) | never — client dispatch (`docs/dev/app/core/launch.md`) |
| `tuxgt launch --apply` / store launch-config write | `apply.md` |
| Harvest body | `apply.md` |
| Type-specific staging rewrites (e.g. OptiScaler absolute ini paths) | later, per ModType |
| GUI Play | `docs/dev/app/gui/game.md` |
| GUI Mods | `docs/dev/app/gui/game.md` |
| GUI adapter choice | Launch Mode card row, persisted per game in `games.adapter` (preload default, scan-safe); changing it runs the all-or-nothing conversion in `mods/adapter.rs` `convert_game_adapter`; Install refused pre-mutation while Hook/Apply is armed or a store client runs |

### Dirs (app sees 3, loader sees 2)

- Depot `<data>/downloads` — provider downloads, immutable, never edited.
- Staging `<game>/stage/<instance>/` — per-game copies plus per-game modifications. The loader's `DepotDir` points at `<game>/stage/`; `LoadDLL`/`IncludeFile` src paths are `<instance>/<rel>`. `<game>` is `games/<l1>/<l2>/` (`game_rel`: l1 = manager or `{manager}_{store}`, l2 = game id with `/` and `:` as `_`).
- Runtime `<game>/runtime/` (`TUXGT_GAME_DIR`) — the loader stages staging→runtime when missing or content differs, prunes staged files the config no longer references on `ok` runs, and leaves generated files. Disable moves that instance's runtime dests and its generated files to `<game>/disabled/<instance>/runtime/` (a dest another enabled instance claims stays, including when this instance's globs also match; a generated file another enabled instance's globs match stays) and enable moves them back, so a launch no longer sees a disabled mod. When both park sides hold the same generated file, the copy from the adapter's live root wins. Uninstall deletes that park and this instance's generated files that no remaining instance claims, moving a live or parked generated file onto the remaining claimant (an enabled claimant's live root, or a disabled claimant's park), plus runtime dests no enabled instance still claims (a dest only a disabled instance still has enabled moves into that instance's park). An emptied `runtime/` directory is removed. The user may delete runtime files; they re-stage on next launch.

Managed ini: `<game>/tuxgt-launcher.ini` (`[Init] GamesDir=runtime DepotDir=stage`). Play/Apply export `TUXGT_LAUNCHER_INI`, `TUXGT_GAME_DIR` (`<game>/runtime`), `TUXGT_DEPOT` (`<game>/stage`) on the manual owned path. Client dispatch argv stays empty; the session file carries those keys for the hook/trampoline when handle is on.

### `staging.toml`

Sibling `<game>/stage/<instance>.staging.toml`, one entry per staged rel path:

```
[files."<rel>"]
depot_sha = "…"       # hash of the depot source at copy time
staged_sha = "…"      # hash after TuxGT modifications (v1: same copy; tuxgt_modified=false)
tuxgt_modified = false
```

Sync rule at install/enable: current hash == `staged_sha` → in sync (re-copy when the depot hash moved, then re-record). Current != `staged_sha` → user-touched → do not overwrite; `instance status` reports it. `--force` re-copies from the depot regardless.

### Prewire

`instance install` / `enable` / `disable` / `uninstall` rewrite `[<stem>]` in the per-game managed ini from GameInfo + enabled manifests. Stem = file stem of the launch exe (store/override); the loader matches sections case-insensitively. `Type` is informational (`{api}_{bitness}` when known). Enabled **preload** manifests contribute **kept** dests only (`[[files]].enabled`, missing = true; required dests in `download.md`). A kept applicable `.dll` (not a `pfx:` copy) is `LoadDLL` unless recipe `include` covers it or `[[files]].load` is `false`, and `load = true` forces `LoadDLL` even when `include` covers it. Every other listed dest, including `.addon64`, is `IncludeFile`. Prefix copies are not listed. `load` does not change slot claim or required-ness (`download.md`). Omitted dests are not listed and not staged / not copied by the install adapter. IncludeFile trees (`dir/=`) unconditionally: one entry per top-level dest dir with any kept file, however many siblings are omitted — omission lives in staging (omitted dests are staging-absent; hand-dropped files intentionally ride the tree and are reported by stage status), never in the ini. Only enabled manifests. Steam rows keep an `[Init]`-only ini (Steam launch options are Apply, `apply.md`). Plans merge per manifest; mixed preload+install for one game is valid, never an error. GUI is keep switches and a Load switch on applicable dll rows on the Mods card, not a raw ini editor. Do not edit `src/launcher/*.c`.

An `include` entry ending in `/` covers the whole dest dir (`shaders/` covers `shaders/foo.fx`); covered DLLs list as `IncludeFile` unless that dest's `load` override says LoadDLL. Manifests iterate in (`load_order`, instance-id) order; a dest claimed twice is emitted twice and the later line wins at stage/load time.

### Play env (on paths we exec)

`build_launch_spec` exports the managed-ini values above, then merges env: store `env` JSON first, then **enabled** global knobs, then **enabled** knobs set on that game (skip disabled — inherit; skip knobs whose provider is disabled), then `env_custom` pairs. Later wins; no revalidation of values or scopes.

`proton_env` plan: set `PROTON_USE_OPTISCALER=1` only when an enabled manifest's official recipe lists `proton_env` and the proton flavor name-matches (lowercased proton string or dir contains `cachy` or `ge-proton`). Shipped `optiscaler` and `optiscaler-y4my4m-v4` do not list it, so a manual OptiScaler install does not set the variable. Its outputs stay in the prefix, unmanaged.

Mod env: after `env_custom`, merge enabled manifests' `[[env]]` in instance-id order (generic keys last-wins). `WINEDLLOVERRIDES` merges per stem (`;`-separated `dll[,dll]=mode`; `n` native, `b` builtin, `n,b` native-first fallback, empty disabled; case-insensitive): existing stems win, mods add missing stems only, first manifest wins per stem. Never rewrites another stem's mode.

Extra `PRESSURE_VESSEL_FILESYSTEMS_RW`: append `install_dir` and `prefix` when set (the wrapper script already covers the game dir, depot, and ini dir).

### Install adapter

`instance install --adapter install|preload` (flag optional; omitted, the game's persisted `games.adapter` choice decides, default `preload`) records `manifest.adapter`. The install adapter copies staging files to the game dir (`install_dir`, else the exe parent) at install/enable time. Dests with the literal ASCII prefix `pfx:windows/system32/<file>` or `pfx:windows/syswow64/<file>` instead copy into the game's Proton/Wine prefix at the Wine `drive_c` (per-game prefix only, never the Proton tree): `pfx:windows/system32/foo.dll` lands at `<prefix>/drive_c/windows/system32/foo.dll` for WINEPREFIX-shaped prefixes, or `<prefix>/pfx/drive_c/windows/system32/foo.dll` when the stored prefix is a Steam `compatdata/<appid>` dir. Only those two roots; a missing prefix (native game) is an install error. User recipes that put a forbidden proxy stem under `pfx:` (the foreign-stem list below, which already covers `dxgi`/`d3d9`/`d3d10`/`d3d11`/`d3d12`) are install errors. Prefix dests are install-adapter only, never preload; prewire never `LoadDLL`s them, so they never consume the 8192-byte ini budget.

Foreign confirm: a game-dir dest whose stem (lowercased, sans `.dll`) is in `{dxgi, d3d9, d3d10, d3d11, d3d12, dxvk, d8, d9, enb, specialk, vkd3d, winmm, version}` and whose existing content is not TuxGT-tracked requires confirm (print the dests, require `y`, or `--yes`; non-TTY without `--yes` is an error). Other game-dir dests overwrite with a backup. Overwritten prefix files get the same backup treatment (Wine builtins included); uninstall restores them. A dest claimed by two enabled install-adapter manifests is a conflict, not an error: last in load order wins, backups keep originals. Reorder re-applies enabled install-adapter copies top-to-bottom in the new order (as `--yes`, backups kept), then prewires. Conflict groups key on (adapter, dest): preload and install roots never collide with each other.

Instance slot: `set_instance_slot` rewrites the claiming Load dest. A slot-named sibling wins; otherwise the single named injector (`OptiScaler.dll`, `ReShade64.dll`, or `ReShade32.dll`) wins over companion DLLs; otherwise a lone sibling. Two injectors, or no single claimer, is an error when a slot was chosen (`no single injector DLL`), not a silent stock-name install. `<self>` keeps the source basename (`OptiScaler.dll`, `ReShade64.dll`); a proxy stem (`dxgi`, `d3d9`, `d3d10`, `d3d11`, `d3d12`, `winmm`, `version`, with or without `.dll`) becomes `<stem>.dll`. A dest another enabled mod already holds is `Error::SlotInUse` (`{instance}: slot {slot} is used by {holder}`) before the same-dest no-op. Disabled mods do not hold a slot. `<self>` that resolves to a proxy-named basename is checked the same way. Install-adapter foreign game-dir dests confirm first. Prints `game<TAB>instance<TAB>slot`. ReShade, OptiScaler, and a custom recipe that names a slot are slot-configurable. NVIDIA Streamline (custom, no `slot`, one dest per DLL in `include`) is not: each DLL stays its own name on preload, Install, and both conversions. Addons, shaders, and textures are not either.

Disable moves that instance's runtime dests and generated files to `<game>/disabled/<instance>/` and enable moves them back onto the live root (runtime for preload; game dir for install). Install-adapter disable also removes tracked game-dir dests and restores backups, and parks generated files that were in the game dir; those dest bytes stay in staging. Uninstall deletes the park, generated files no remaining instance claims, install-adapter game-dir copies, staging, runtime dests, and the manifest, then rewrites `[Stem]`. A non-generated dest whose hash no longer matches tracked content is foreign and left alone; the uninstall confirm names only those. A generated dest such as OptiScaler.ini whose bytes diverged is parked with the other generated files and, on uninstall, deleted or handed to a remaining claimant. A backup `remove_copies` just restored stays in the game dir. A same-named file still under `runtime/` is parked on disable and handed off or removed on uninstall.

Play sets `WINEDLLOVERRIDES` `<stem>=n,b` entries (`;`-joined, merged with any existing value) for install-manifest `*.dll` dests on proton/wine paths, except a stock `<self>` name (`OptiScaler.dll`, `ReShade64.dll`, `ReShade32.dll`). That merge also covers generic `pfx:` dll dests; a proxied stem may use a `=n` override. A claiming dest that is a proxy stem (`dxgi`, `d3d9`, `d3d10`, `d3d11`, `d3d12`, `winmm`, `version`) always adds `<stem>=n,b` on Play and in `tux-protonfixes.conf`, for preload and install. `<self>` adds nothing. Existing stems win, including a stem already in the store env or launch options and a grouped `dll,dll=mode` entry. The session value is what the trampoline exports, so it includes those install-adapter companion stems too. In `tux-protonfixes.conf` a value that is not one shell word is double-quoted, and the trampoline and the protonfixes hook both decode it to the raw value.

### CLI

```
tuxgt instance install <game> <mod> [--adapter preload|install] [--slot <self|dxgi|d3d9|d3d10|d3d11|d3d12|winmm|version>] [--redownload] [--yes] [--force]
tuxgt instance uninstall <game> <mod> [--yes]
tuxgt instance status <game>
tuxgt instance files <game> <mod>
tuxgt instance files <game> <mod> enable <dest> [--yes]
tuxgt instance files <game> <mod> disable <dest> [--yes]
tuxgt instance files <game> <mod> loaddll <dest>
tuxgt instance files <game> <mod> include <dest>
tuxgt instance slot <game> <mod> <self|dxgi|d3d9|d3d10|d3d11|d3d12|winmm|version> [--yes]
tuxgt instance check <game> [<mod>] [--yes] [--slot <self|dxgi|d3d9|d3d10|d3d11|d3d12|winmm|version>]
tuxgt instance update <game> [<mod>] [--yes] [--slot <self|dxgi|d3d9|d3d10|d3d11|d3d12|winmm|version>]
tuxgt instance resync <game> [<mod>] [--yes]
tuxgt games adapter <id>
tuxgt games adapter <id> preload|install [--yes] [--slot <self|dxgi|d3d9|d3d10|d3d11|d3d12|winmm|version>]
```

`--yes` answers the foreign-dest confirm and the store-client stop/restart. `--force` re-copies user-touched staging from the depot. `--slot` names the claiming dest for a slot-configurable Install-adapter install (`<self>` or a proxy stem). Without it that install returns `NeedSlotChoice` before any download; a stem another enabled mod holds returns `SlotInUse`. Preload installs keep the stock basename. `status` prints `mod<TAB>file<TAB>in-sync|user-modified|depot-newer` per staged file and exits 0. `files loaddll|include` writes `[[files]].load`. `check` repairs provenance/cache (GUI baseline), then prints `up-to-date|available|unknown`. `update` checks, then redownloads on confirmation when available. `resync` is Force re-sync (overwrites user-touched staging); `--yes` skips the shared-proxy re-pick. `games adapter` prints or converts the persisted `games.adapter` choice (same `convert_game_adapter` as the GUI). Convert→Install consents first, then unhooks, then auto-Applies; Convert→Preload restores. A running Steam/Heroic prompts to stop, write, and restart (same as the GUI ClientStop card).


### Persisted choice and conversion

`games.adapter` holds one validated choice per game (`preload` | `install`); the schema
default backfills legacy rows and the scan upsert never writes it. `game_adapter` /
`set_game_adapter` are the only accessors; `InstallOpts.adapter` is `Option<String>`, so
`None` (every GUI path) resolves the stored choice at install time and only the CLI flag
overrides per call.

`convert_game_adapter(pool, data, config, game, target, yes, slots_chosen)` is all-or-nothing for the move itself:

- refuses before any mutation when a recipe disallows the target, a recipe is missing, or
  an install-adapter foreign game-dir overwrite lacks `yes` (`Error::NeedConfirm`,
  `adapter-convert: <dests>`);
- when `slots_chosen` is false and a slot-configurable mod is moving, returns
  `Error::NeedSlotChoice` (`need-slot: <instances>`) before any rename or copy. Touched
  staging on the current claiming dest still refuses first (`StagedModified`). To
  `install`, every slot-configurable mod is in that list. To `preload`, only one whose
  claiming dest is already a proxy name, so the GUI can offer a rename back to `<self>`
  (default yes). NVIDIA Streamline, addons, shaders, and textures are never in the list;
  each of their DLLs keeps its own basename. CLI `games adapter` converts the same way (`--slot` fills every named instance); a running store client confirms stop-write-restart;
- the GUI records each pick before the conversion snapshot, then replays with
  `slots_chosen` true. That replay does not ask again, including when a later
  `NeedConfirm` retries from the Overwrite card (`slots_chosen` stays true). Preload
  picks are manifest-only (`set_instance_slot`) and happen before the stop. Install-to-preload
  picks call `unplace_instance_slot` only after the client stop is confirmed: the old
  tracked game-dir copy comes out, the new name is not copied in, and Cancel of that
  stop writes nothing. A disabled mod's pick is manifest-only, so its DLL is not copied
  back. The conversion itself does not rename dests. A pick is outside the conversion
  snapshot: a rolled-back conversion keeps the picked dest and the old adapter;
- a fresh Install-adapter install of a slot-configurable mod with no prior manifest and
  no `InstallOpts.slot` returns `NeedSlotChoice` before download. The GUI default is the
  recipe slot (`dxgi` when the recipe names none) and `<self>` is in the list, with a
  warning that the game will not load a stock-named DLL. Preload installs keep the stock
  basename. A recipe `slot = "dxgi"` is only that Install-prompt default. `OptiScaler.dll`
  is not rewritten to `dxgi.dll`;
- a re-sync does not fail when two enabled mods already share a proxy dest. The GUI
  offers those slot-configurable mods a re-pick. Confirming an unchanged clash reports
  `SlotInUse` and does not reopen the card. Cancel drops the card and leaves the files;
- carries runtime-generated (harvested) files from the old root to the new one (game dir
  on install, `<game>/runtime/` on preload) so user settings follow the mod; a rel present
  with differing bytes at both roots joins the single `NeedConfirm` consent set
  (`adapter-convert: <dests>`), and the live side wins once consented, while identical
  collisions dedupe silently;
- snapshots game-dir/prefix dests, the backup dir, the manifests, the prewire ini, and the
  stored choice after any slot pick; applies restages, manifests + copies, harvested moves,
  regenerates prewire, and writes `games.adapter` **last**;
- on any failure restores every one of those (harvested moves restore both roots) before
  returning. A rolled-back game keeps its old adapter and the dest the pick already wrote.

The GUI (`gui/game/launch.rs`) paints the stored value on every game row (manual included),
refuses Install while Hook/Apply is armed or a store client runs, and re-reads core after
every result. The two consents stay separate: a direct click authorizes neither, the
ClientStop card authorizes only the stop, and a foreign game-dir overwrite raises the
standard Overwrite card (Confirm retries with `yes` and the same `slots_chosen`). Cancel of
that card does not copy into the game dir; a slot pick applied before it stays. A
`NeedSlotChoice` parks a per-mod dropdown (`<self>` plus the five proxy names) before the
stop. Install-to-preload applies those picks only after that stop is confirmed. Plain Play
is untouched and stays vanilla.

`kill -9` mid-conversion is not covered: a graceful failure restores the conversion snapshot
(tested at three injection points) and leaves the already-applied slot pick in place. A
durable journal/resume step is a separate design.
