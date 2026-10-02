## LaunchSpec + Play

Types may gain fields; not an external ABI.

Not in this surface:

| Item | Where |
|---|---|
| `LaunchAdapter` preload vs install | `launch-adapter.md` |
| Env as a plan on the injector | `launch-adapter.md` |
| Prewire `[Stem]` / per-game `TUXGT_GAME_DIR` (`<game>/runtime/`) | `launch-adapter.md` |
| Extra `PRESSURE_VESSEL_FILESYSTEMS_RW` (beyond `src/launcher/tuxgt-launcher`) | `launch-adapter.md` |
| EnvKnob merge into spec | this file + `env-knob.md` |
| `tuxgt launch --apply` / store launch-config write | `apply.md` |
| GUI Play | `docs/dev/app/gui/game.md` (Launch Mode on General; Enable & Play; manual injects) |
| Heroic dispatch | `heroic "heroic://launch?appName=<id>&runner=<runner>"`; tuxgt never execs Heroic exes — Heroic owns Wine/Proton; handle+session inject on GE/Cachy |
| Wrap `tuxgt-launcher` around `steam steam://rungameid/…` | never; Apply writes Steam launch options (`apply.md`); session is the env channel |
| Handle + session + protonfixes hook | this file; `docs/dev/app/core/launch.md` |

Core rebuilds `LaunchSpec` on each Play from the sqlx row. Not persisted. Enable/install writes `FileManifest` only; the next Play re-resolves.

Launch target is **store** (override exe if set, else `exe_path`). Detected exe is not launched.

No sqlx write. No detector run.

### Shape

```
LaunchSpec:
  id: GameId
  cwd: PathBuf
  env: BTreeMap<String, String>
  wrappers: Vec<Vec<String>>   # outer-first; each is program + args wrapping the remainder
  program: PathBuf             # innermost runner or exe (or the store client binary on dispatch paths)
  args: Vec<String>            # exe args, or the single client URL on dispatch paths
  owned: bool                  # false on Steam/Heroic dispatch (no wrappers/env-merge/harvest-wait); true on manual
```

`argv()` = fold wrappers inside-out onto `program + args`.

### Runner

| manager | innermost |
|---|---|
| `steam` | `steam steam://rungameid/<CGameID>`. Owned: CGameID = appid. Shortcut: (vdf u32 << 32) bit-or 0x02000000. No wrappers. Steam missing → error. Spec `owned: false`. |
| `manual` | store/override exe via `platform` (below). The only owned path (`owned: true`). |
| `heroic` | `heroic "heroic://launch?appName=<game_id>&runner=<runner>"` (runner omitted unless `gog`/`standalone`→`sideload`/`legendary`/`nile`; those are Heroic's wire names). No wrappers, no env merge. Heroic missing → error. Spec `owned: false`. |

`steam` lookup: `PATH`, then `<steamlocate root>/steam.sh`. `heroic` lookup: native `heroic` on `PATH`. Flatpak dispatch is not planned.

### Handle + session

Hook channel per game (`game_handle.inject`, default off; Launch Mode radio arm). Not a mod. CLI `tuxgt games handle <id> [--on|--off]`; `--on` while Applied restores the trampoline first (mutual exclusion, `apply.md`). An arm nothing needs never persists (`launch.md` auto-restore): `--on` on a game with no preload/env/argv-wrapper/install need is cleared by the same `sync_session`, and the command reports the resulting state (`off`). Automatic Apply restore defers while the owning store client runs; hook-only needs-gone disarm does not (handle is PREFIX-local).

`games/<rel>/tux-protonfixes.conf` rewritten by `sync_session` on handle, knob, custom env, wrapper, and prewire (`rel` is `game_rel`: `{manager}_{store}/{id}` or `{manager}/{id}`). `games/load-correlator.ini` is rewritten then and after exe/prefix override, doctor, scan, and GUI library load:

```
inject=1
preload=1
TUXGT_LAUNCHER_INI=...
TUXGT_GAME_DIR=...
TUXGT_DEPOT=...
TUXGT_LAUNCHER_SO=.../libtuxgt-launcher.so
DXVK_HUD=1
WRAPPERS=gamescope,gamemode
```

Unset and **disabled** knobs omitted (inherit). Enabled globals merge in before per-game knobs; unset/disabled omitted so the game inherits global or process unmanaged. `WRAPPERS=` is trampoline-only (protonfixes ignores it). `preload=1` is written only when an enabled instance uses the preload adapter; the trampoline skips `LD_PRELOAD` of the loader without it (protonfixes ignores `preload`). Applied sessions carry `inject=0` so the hook no-ops; the trampoline self-arms from argv. The file never carries `LD_PRELOAD=` — the hook (`tuxgt_apply.py`) and the trampoline (`src/launcher/tuxgt-launcher`) append it from `TUXGT_LAUNCHER_SO` at launch time when loading the `.so`. `TUXGT_LAUNCHER_SO` stays the per-game primary (`libtuxgt-launcher32.so` when that file exists for a 32-bit game, else the 64-bit SO). The same apply path also appends the sibling ELF class next to it when that file exists (`libtuxgt-launcher.so` ↔ `libtuxgt-launcher32.so`); ld.so loads the matching class. WoW64 Proton (64-bit Unix wine running a 32-bit PE) needs both. `PRESSURE_VESSEL_FILESYSTEMS_RW` still grants the primary SO's parent (the sibling shares it).

`PROTON_USE_OPTISCALER=1` is granted under the same conditions as the owned env plan (enabled manifest whose official recipe allows `proton_env`, plus platform resolving to `proton` with a CachyOS/GE flavor). Shipped OptiScaler recipes do not list `proton_env`. The hook and the Apply trampoline export it, so store rows get the grant without a `LaunchSpec`. A proxy slot (`dxgi`, `d3d9`, `d3d10`, `d3d11`, `d3d12`, `winmm`, `version`) adds `WINEDLLOVERRIDES` `<stem>=n,b` to the same file; `<self>` does not. Existing stems win.

Lookup is `games/load-correlator.ini`: section is the prefix path, keys are exe paths, value is `games` rel (`map[prefix][exe]`). The hook/trampoline take prefix from `STEAM_COMPAT_DATA_PATH` else `WINEPREFIX`, exe from `EXE` else the last `.exe` argv. Exact miss → prefix fallback: a prefix section naming exactly one rel resolves to it, so launcher-first chains (MO2 outer, SKSE middle) hit the session and the inner game exe inherits the env; the loader stem gate stays the decider. Contested prefixes and unknown prefixes miss. No `run/<key>`, no one-file fallback. A game missing prefix or exe in the index is omitted.

Extra correlator exes: one game may own several exe keys under the same prefix section, all mapping to its rel — a primary plus user-added extras (`game_extra_exes` sidecar, prefix always from the row's effective `prefix_path` at render time). A game with a primary plus one extra renders two `exe=rel` keys under one `[prefix]` section, so launching via either Proton-reported exe hits the same session. CLI `tuxgt games extra-exe <id> add|remove|list`; mutating writers persist then call `sync_session` (same sequence as `set_override` → `sync_session`), so handle/knob/wrapper/prewire edits keep extras for free. An extra equal to the primary after normalization is skipped; a prefix/exe-less game is omitted extras included. Detector never writes extras (`detect/mo2.rs` keeps storing exactly one `detected_exe_path`, the inner game exe): the MO2 flow is user-added — Steam row stores `ModOrganizer.exe`, detection stores the inner exe, and the other side is added as an extra, so loader-exe and inner-exe launches correlate to one rel. Extras widen hook/trampoline correlation only, never the Play target.

Keys are canonical on both sides: backslash → slash, lowercase, trim (`session::canonical_exe_key` on write for primaries and extras alike; hook `tuxgt_apply.py` `_norm` and trampoline `norm_path`/`correlate_rel` on lookup), so a mixed-case Proton-reported `SkyrimSE.exe` hits a lowercased ini key.

Collisions: a user-initiated write (extra add, exe/prefix override via doctor `--set` or the Launch-tab editors) whose post-write render would land on another game's `(prefix, exe)` key is refused with an error naming the owning game and the key, and writes nothing (no partial write). Batch paths (scan, store resync, library load) never refuse — they keep writing and the render stays deterministic and loud: first rel in sort order wins a contested key, losers are dropped, and a `tracing::warn!` names both games (never silent last-wins).

protonfixes `localfixes/default.py` (GE/Cachy only) chains packaged global default unless wrapping a foreign user `default.py`, then `tuxgt.apply()`: missing/`inject!=1` → return; else `util.set_environment` for every other pair.

Non-Steam `platform` empty + (prefix or proton) → `proton`; else `native`.

| platform | innermost (manual) |
|---|---|
| `native` | exe |
| `proton` | `umu-run` if found, else `{proton}/proton waitforexitandrun` (proton field as path, or Steam `compatibilitytools.d` / `steamapps/common`) |
| `wine` | `wine` on PATH, or proton field if it is a file |

(Manual rows only. Steam/Heroic dispatch to their clients — no runner, no wrappers.)

### Wrappers (manual rows; never Steam/Heroic)
- Manual-row `launch_options`: if `%command%` present, tokens before it are one wrapper argv (plus `KEY=VAL` → env); tokens after are extra exe args. No `%command%` → all extra args. (Steam's own options stay Steam's; Heroic's `wrapper`/`launcherArgs` live in GamesConfig and are applied by the client, or by Apply.)
- Store env JSON merged first, then enabled global knobs, then enabled game knobs, then `env_custom`; Play then sets `STEAM_COMPAT_*` / `GAMEID` / `WINEPREFIX` / `PROTONPATH` on proton/wine paths we exec. Skip disabled (inherit).
- Append `tuxgt-launcher` as the last wrapper unless a wrapper token already has that basename.
- Overlay wrappers stored for the game (`game_wrappers`, `wrapper.md`) are appended before `tuxgt-launcher` in table order (gamescope → gamemoderun → mangohud); one whose program basename is already composed is skipped.
- Do not drop existing `mangohud` / `gamemoderun` / other store wrappers.

`tuxgt-launcher` lookup: `$TUXGT_DATA/tuxgt-launcher`, `$TUXGT_DATA/bin/tuxgt-launcher`, next to the `tuxgt` binary, then `PATH`. Missing → error on paths that wrap it. The script still appends `LD_PRELOAD` and `PRESSURE_VESSEL_FILESYSTEMS_RW`.

Proton env when we exec: `STEAM_COMPAT_DATA_PATH` (prefix), `STEAM_COMPAT_INSTALL_PATH` (install_dir), `STEAM_COMPAT_APP_ID` (`0` here), `STEAM_COMPAT_CLIENT_INSTALL_PATH` when Steam is locatable, `GAMEID`=`umu-default`. `PROTONPATH` when a proton dir was resolved. Wine: `WINEPREFIX` when prefix is set.

No store/override exe on a manual row → error. Proton/wine with no runner → error. 32-bit vanilla Play is allowed. Detector does not inject 32-bit; refuse is later.

### CLI

```
tuxgt launch [--print] <id>
```

Unknown id → error. `--print`: write `argv` as one shell-quoted line to stdout; do not exec. Without `--print`: exec (Unix replace). Neither writes the db.

