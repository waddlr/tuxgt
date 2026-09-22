# Troubleshooting

If something looks wrong, start here. For anything still broken, open `$PREFIX/logs/tuxgt.log` (the latest run; prior runs are `tuxgt.log.YYYYMMDD-HHMMSS`, `-<pid>` on same-second collisions, newest 5 kept) and include the relevant lines when asking for help.

## Known issues carried from the upstream tracker

These are known issues that are not fixed yet.

### Tooltip sticks after you go Back / switch page (R65 — user-facing, still true)

**Symptom:** You hover a button so its tooltip shows, then press the mouse Back button (or Forward) to change page. The old tooltip stays on screen, floating over the new page, until the next tooltip appears.

**Why:** The tooltip widget only hides on left mouse-down (`MouseButton::Left`). Navigation via a non-left button unmounts the trigger without a hover-exit, so the overlay never gets the hide signal. Any tooltip on any page can do this — left-click, right-click, scroll, and mouse-move do not cause it. The app cannot force-clear it (`Root::tooltip_overlay` is `pub(crate)` with no public dismiss API) and the same code is in the latest stable and git HEAD of `longbridge/gpui-kit`.

**Fix / workaround:** Move the mouse a little — the next tooltip will clear it. To fix properly, the toolkit needs a patch to hide on all mouse buttons (or expose a dismiss API).

### Headless lane: app panics on restart after kill (R67 — developer / lab only, not normal desktop use)

**Symptom:** In a headless GUI lane (the `src/tuxgt/tools/gui-session` sway fixture), if you kill `tuxgt gui` and start it again, it panics in the Wayland event loop (`client.rs:1829` unwrap on `None` keymap_state — Modifiers arrived before a valid keymap) and the window never appears. Every restart panics until you stop and start the whole lane.

**Why:** The lane seat uses wlroots `wlinput` persistent virtual keyboard whose compositor-side state survives app restarts. Real desktops always supply a keymap, so this never happens there.

**Fix / workaround:** Stop and start the whole lane instead of just restarting the app. Not needed on a normal desktop; CLI/core and real game sessions are unaffected. To fix properly, vendor-patch the toolkit guard or capture `WAYLAND_DEBUG=1` on restart and patch/lane-tool the wlinput trigger if confirmed.

## Common problems

### 1. Game does not appear in the Library

**Symptoms:** You installed a new Steam or Heroic game but TuxGT's Library is empty for it, or shows an old list.

**Fix:**

- In the app, use **Settings → General → Rescan libraries** to re-run detectors (the same as `tuxgt scan` in a terminal). If overrides are set, the app asks for confirmation; `tuxgt scan --force --yes` re-runs all detectors and clears overrides without prompting.
- Check that the provider is not hidden or disabled in the Library filters. Hidden games (Steam `localconfig`/`sharedconfig` hidden flag, Heroic `store/config.json` `games.hidden`) are filtered by default.
- Make sure Steam or Heroic is installed where TuxGT looks (`steamlocate` for Steam, `~/.config/heroic` and friends for Heroic). Flatpak Heroic is discovered too.
- Look in `~/.config/tuxgt.conf` — `TUXGT_DATA` must point at the prefix that holds `config/tuxgt.sqlite`. If you moved the tree, follow the "Updating / relocating" steps in [Install](install.md) (`mv <old> <new> && <new>/bin/tuxgt install`).

### 2. Mod installed but nothing shows in game

**Symptoms:** You enabled OptiScaler/ReShade or a shader pack, pressed **Play**, and the game runs vanilla.

**Fix — check Launch Mode on the Game page:**

- **Hook protonfixes** only works when the game's Proton is GE or CachyOS (this path keeps the game folder untouched). If the game uses Valve Proton or for Heroic, pick **Update Launch Options** instead.
- **Update Launch Options** needs one restart of Steam or Heroic after you arm it — both clients cache `localconfig.vdf` / `GamesConfig` at startup, so an Apply written while it is running is dropped by the next settings save. The app toasts "Heroic is running: restart it…". Apply writes the trampoline (`tuxgt-launcher` wrapper) into the store config; then `steam steam://rungameid/<…>` or `heroic heroic://launch?...` runs the trampoline, which re-applies `WRAPPERS` and `LD_PRELOAD` even at `inject=0`.
- **Not hooked** is the plain store path — it always leaves mods inert so you can test vanilla without uninstalling. If the radio shows Not hooked but you expect mods, arm one of the other two.
- If you also turned on an outer wrapper like **gamescope**, **GameMode** (`gamemoderun`), or **MangoHud** as an argv wrapper, the app auto-switches Hook → Update Launch Options — that is required (the hook cannot wrap argv wrappers). Both Store rows behave the same.
- Verify with `tuxgt install --check` that host hooks are still `ok` — a missing `localfixes/default.py` marker shows as `modified`/`missing` and means reinstall (`./tuxgt/bin/tuxgt install --yes`).

### 3. Host files report missing or modified (`tuxgt install --check` not all `ok`)

**Symptoms:** You see lines like `~/.local/bin/tuxgt  wrong-target` or `icons  7/9  modified` and the final line is not `ok  <total>/<total>`.

**Fix:**

- Run the packaged installer again from the unpacked tree: `./tuxgt/bin/tuxgt install` (or `--yes`). It overwrites intended files from the baked copy and reconciles stale owned ones. Files you changed yourself where the sha/marker no longer matches the old inventory are left and reported as `skipped`.
- The nine themed icons collapse to one `icons  n/9` line — treat "7/9" as "two icons still not ours".
- If the boot prefix is wrong, check `tuxgt install --check --prefix /path/to/prefix` to see which tree is failing.

### 4. "This package requires reshade" / slot conflict errors

**Symptoms:**

- Enabling a ReShade addon/shader/texture shows "This package requires reshade. Install that instance first?" or after confirming, "Missing required instance of type reshade".
- Two mods both want `dxgi.dll` (or `d3d11.dll`) and the UI shows a conflict row with rivals.

**Fix:**

- Install the required instance first. OptiScaler and ReShade are independent; but `reshade_addon`, `effect`, and `texture` types always need a ReShade instance for that game. The app offers **Install required** — accept it, or run `tuxgt instance install <game> reshade`.
- For slot conflicts (`conflicts: [{ slot: dxgi }]`), use **Slot** on the installed card to pick a different proxy slot (`dxgi`, `d3d11`, `d3d12`, `winmm`, `version`) or **Make win** / the move arrows in **Load conflicts** to decide which last wins. The graph diagnostic `tuxgt mods graph` prints `conflicts  ok` / `slot_conflicts` for the fixture set.

### 5. Downloads fail or stall

**Symptoms:** Installing an Instance stays on "fetching…" or finishes with a TLS/sha256 error; the cache under `$PREFIX/downloads/` stays with a `.part`.

**Fix:**

- TuxGT uses `reqwest` with `rustls` and full TLS verification — no `--insecure`. If TLS fails, check system time and that you can `curl https://github.com`.
- GitHub asset fetches use the Release API with `asset_glob` matching (for example `OptiScaler_*.7z`). A rate limit exits with a GitHub error — wait a minute or set a GitHub token via your system keyring if you fetch a lot.
- Resume is via `Range`: an interrupted download leaves `$PREFIX/downloads/<hash>/… .part` and resumes from that size on 206 responses; a short body before `Content-Length` is treated as an error and kept for resume, not hashed. Re-run the install — it will resume.
- Pinned `sha256` in a recipe is enforced — a mismatch errors after download so unverified bytes are never installed. Without a pin, the first fetch records its hash in `meta.toml` and later mismatches re-fetch automatically. Use `tuxgt cache refresh [instance]` or delete the payload dir and reinstall to force a redownload.

### 6. Uninstall left files or `tuxgt uninstall` says no inventory

**Symptoms:** `tuxgt uninstall` prints `skipped  …` instead of `removed`, or errors "no host inventory at …: run `tuxgt install` once".

**Fix:**

- `tuxgt uninstall` only removes what is listed in `$PREFIX/config/host-install.toml` and only while we still own it (symlink points at our target, file sha256 matches the inventory, or hook marker `tuxgt-proton-hook v1` is present). Files you edited after install are deliberately left — that "skipped" is not a bug. Restore your edits or re-run `tuxgt install` to reclaim them before uninstalling.
- "no inventory" means you never ran `tuxgt install` for this prefix (or you deleted `config/host-install.toml`). Run `./tuxgt/bin/tuxgt install --yes` once to create the inventory, then `tuxgt uninstall --yes` will work.
- Only files and symlinks are ever removed; directories (including `$PREFIX`, `games/`, `mods/user/`, `downloads/`, `config/tuxgt.sqlite`) are never deleted — remove them by hand with `rm -rf $PREFIX` after uninstall if you want the data gone.
