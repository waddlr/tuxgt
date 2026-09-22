## Apply + harvest

Optional Apply so the store Play button injects; harvest of runtime-generated files into the manifest. GUI Launch Mode radio calls the same `apply_launch` / `restore_launch`. No launcher C rewrite. Apply is one arm of Hook XOR Apply; `inject=` is hook-only and the trampoline self-arms from argv.

Not in this surface:

| Item | Where |
|---|---|
| GUI Apply / Restore | Launch Mode radio; same core APIs |
| Store launch-config **read** (options, env, wrapper, Proton pick) | `detector.md` |
| Game-dir install backups / foreign-DLL confirm | `launch-adapter.md` |
| 32-bit Play refuse | later |

### Apply (optional, reversible)

`GameProvider::apply(ctx, game_id)` / `restore(ctx, game_id)` return a one-line report. `ApplyCtx` carries `data_dir`, the `tuxgt-launcher` path, and the per-game managed ini (`<game>/tuxgt-launcher.ini`), game dir (`<game>/runtime`), and depot (`<game>/stage`) dirs (same files Play and prewire use). Default is `ApplyUnsupported`; `manual` never gains a body. Cross-manager calls (steam provider on a heroic id) are `ApplyUnsupported`.

- Atomic write (temp + rename) + backup on every store file touched.
- Idempotent: re-apply reports `already applied`, never duplicates entries.
- Compose, don't clobber: existing user options/wrappers/env are kept; our entries are added around them.
- Record: `<game>/apply.toml` (`game`, `manager`, `launcher`, `applied_at`, `files[]` with per-file `path`, first-wins whole-file `backup`, and `previous` fragment). Restore is surgical per fragment so other games' entries in shared files survive; the whole-file backup is disaster recovery only.
- No record on restore → `Error::Apply` (not a silent no-op).

### Steam

Every existing `<steam-root>/userdata/*/config/localconfig.vdf` (native + Flatpak via `steamlocate`). No such file → `Error::Apply`.

- Sets the app's `LaunchOptions` (owned apps and `steam:standalone:<appid>` share the numeric key): existing options are kept with our launcher inserted before `%command%`; without `%command%` it is appended after our launcher.
- Byte-preserving text surgery on the `apps` object (the file is never re-serialized): replace the `LaunchOptions` line, or create the app block when missing. Missing `apps` section → `Error::Apply`.
- `previous` = the prior `LaunchOptions` (None = key absent → restore removes the line, leaving an emptied block Steam treats as untouched).

### Heroic

`GamesConfig/<app>.json` under the native/Flatpak config roots (first root when creating). No roots → `Error::Apply`. Shape mirrors `launch_snap`: an object keyed by app id is edited in place, else the flat top level; a missing file starts as an empty object (keyed entry created).

- `wrapperOptions` gains `{exe: <launcher>, args: ""}` unless an entry already names the launcher (exact path or same file name).
- Do **not** write `TUXGT_*` or knobs into `enviromentOptions`. The trampoline reads `games/<rel>/tux-protonfixes.conf` via `games/load-correlator.ini`.
- `previous` = snapshot of `wrapperOptions` only (null = key absent → restore removes it). Old records that also snapshotted `enviromentOptions` still restore those keys.

### Mutual exclusion

Selecting **Update Launch Options** → `set_handle(false)` then `apply_launch` (Apply while Handle on clears Handle, then Applies). Selecting **Hook** → `restore_launch` if applied, then `set_handle(true)` (`tuxgt games handle --on` while applied restores the trampoline first). Selecting **Not hooked** → restore if applied + handle off. CLI `tuxgt launch --apply` clears Handle the same way. Applied sessions carry `inject=0` so the hook no-ops; the trampoline self-arms from argv. GUI Enable & Play arms Apply when argv wrappers are on (`launch.needs`); otherwise Hook-if-GE-else-Apply.

### Restore (CLI + Launch Mode radio)

`restore_launch(data_dir, game_id)` writes each recorded `previous` fragment back (or removes the key), then drops the record. Missing target files warn-and-skip; unparseable present files error and keep the record. Core owns this; CLI and the GUI Launch Mode radio share it.

### Harvest

`generated_globs_for(mod_type, dests)`: `reshade` → `ReShade.ini`, `ReShade.log`; `optiscaler` → `OptiScaler.ini`, `OptiScaler.log`; `reshade_addon` → the ReShade pair plus `<stem>.log` per staged `.addon64` dest; else empty. Set at `mods install`; harvest backfills empty globs from the mod type.

- Roots: `harvest_roots` walks the managed `<game>/runtime/` dir always when it exists on disk, plus the install-adapter `game_root` when it resolves and differs. `harvest_game_roots` matches globs against file names under each root (recursive, depth ≤ 4, hidden dirs and symlinks skipped) and merges hits into one `manifest.harvested` map (rel path → sha256; runtime wins on rel collision); stale entries drop when absent from BOTH trees.
- `harvest_game(data_dir, game_id, root)` is a thin single-root wrapper; `harvest_all(pool, data_dir)` harvests every game with manifests via `harvest_roots`; per-game failures warn-and-continue, never fail the scan.
- Triggers: after an owned (manual) Play with manifests (CLI spawns + waits instead of exec, then harvests), on next `tuxgt scan`, and on GUI Rescan (`scan_library` runs the same `harvest_all` path; covers client dispatches, which hand off immediately).
- Uninstall policy: delete tracked dests, restore backups, leave foreign non-generated files, and delete generated files that no remaining instance claims, including ones parked by disable and a generated dest whose bytes no longer match tracked content. A backup just restored stays in the game dir; a same-named file still under `runtime/` is handed off or removed. The uninstall confirm names only non-generated foreign dests. Adapter conversion still carries harvested files to the new root (`launch-adapter.md`).

### CLI

```
tuxgt launch --apply <id>     # clear Handle, persist wrapper into Steam/Heroic, then play
tuxgt launch --restore <id>   # restore pre-Apply store config, do not play
```

`--apply` + `--restore` is an error. Play prints `harvest<TAB><path>` lines for harvested files; `scan` prints `harvest<TAB><game><TAB><path>`.

