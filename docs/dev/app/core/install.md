# Install and source tree

## Userland install only

No Flatpak, no AppImage, no `/usr`, no sudo.

Ship `dist/tuxgt.tar.gz` (`make package`): unpacks to a self-contained `tuxgt/` tree. Then `tuxgt/bin/tuxgt install` writes the two PATH symlinks, one KDE Games desktop entry, and `~/.config/tuxgt.conf` (boot pointer only). Interactive when a terminal is present (default `$HOME/tuxgt`, `--prefix` skips the prompt, `--yes` accepts the default).

The tarball is the app + launcher + share (`share/templates/*.toml` Add templates, `share/protonfixes/`, `share/icons/hicolor/*/apps/tuxgt.png` (9 sizes)) + official recipe TOMLs at `mods/official/*.toml`. **No payload mods:** no ReShade, no NVStreamline, no first-party `.addon64` binaries. Empty `mods/` / `downloads/` / `config/` / `games/` dirs may exist for layout. `make prepare` fills `build/pkg/tuxgt` from repo files + our artifacts; `package` tars it; `deploy` overlays from it. Neither requires mingw or the addons. ReShade is a runtime download (`docs/dev/app/core/mods.md`, official `reshade` Mod). Payload dirs `mods/official/<id>/` are created on first install, not in the tarball.

`make deploy` overlays **this tree's build** into the live prefix (`TUXGT_DATA` from `~/.config/tuxgt.conf`): `bin/tuxgt`, `bin/tuxgt-launcher`, `lib/libtuxgt-launcher.so`, `share/protonfixes/`, `share/` (desktop file only if missing; `share/templates/*.toml` + `share/icons/` always). Official recipe TOMLs: write/update `$PREFIX/mods/official/*.toml`; delete PREFIX official tomls that left the package **and** drop matching `mods/official/<id>/`. It does not touch `games/`, `downloads/`, `config/`, `mods/user/`, registries, or payload dirs of still-shipped officials. It does not copy payload mods and does not require mingw. Missing conf or empty `TUXGT_DATA` → error (`tuxgt install` first). Not `tuxgt install`; no `make install`.

Local-test payloads only (not the tarball). Same conf path. Missing conf → same error.

Reinstall over an existing prefix (issue #1, decided 2026-10-05): when the dest prefix already holds `bin/tuxgt`, `tuxgt install` overlays this tree's program files first — `bin/`, `lib/`, `share/` (except `share/applications/tuxgt.desktop`, rewritten for the prefix right after), plus `mods/official/*.toml` with deploy's drop-gone rule — then runs the normal host refresh. The silent keep is wrong: a re-run from a newer/different unpacked tree always replaces `bin/tuxgt` (tmp + rename per file, package modes). `games/`, `downloads/`, `config/` (except the inventory itself), `mods/user/`, registries, and kept official payloads are never touched. A dest dir without `bin/tuxgt` still refuses to clobber.

| Target | Into `$TUXGT_DATA` |
|---|---|
| `make deploy-addons` | `mods/tuxgt.addon64`, `mods/tuxgt-nr.addon64` (builds them) |
| `make deploy-nvstreamline` | `mods/NVStreamline/2.13.0/` from `external/nvngx_dlss.dll` + `nvngx_dlssnr.dll` (error if absent) |

```
$PREFIX/
  bin/tuxgt
  bin/tuxgt-launcher
  lib/libtuxgt-launcher.so
  mods/official/<id>.toml          # official recipes (packaged); payload dir beside on first install
  mods/official/<id>/              # official payload (created at install, not in the tarball)
  mods/user/<id>.toml              # user recipes
  mods/user/<id>/                  # user payload (copied on Add)
  mods/<registry>/<id>.toml        # 3rd-party registry recipes (`official`/`user` reserved; writer Later)
  downloads/                       # in-flight fetch only; empty at rest
  config/                          # plugins.toml, ui.toml, mods.toml, tuxgt.sqlite
  config/cache/art/<game-id-safe>/cover
  share/applications/tuxgt.desktop
  share/icons/hicolor/<16..1024>/apps/tuxgt.png  # 9 sizes, themed `Icon=tuxgt`
  logs/                            # `<data>/logs/tuxgt.log.<date>` (rolling daily, first run)
  share/templates/*.toml            # Add templates
  share/protonfixes/                # tuxgt_apply.py + default.py; localfixes is a shim
  games/load-correlator.ini        # prefix+exe → <l1>/<l2>
  games/<l1>/<l2>/                 # l1 = manager or {manager}_{store}; l2 = game id
    tux-protonfixes.conf           # handle/env/WRAPPERS for the hook + trampoline
    tuxgt-launcher.ini             # managed ini ([Init] GamesDir=runtime DepotDir=stage)
    stage/<instance>/              # app staging
    runtime/                       # loader dest: DLLs, ReShade.ini, logs
    manifests/<instance>.toml
    apply.toml
    backups/
```

Only host files outside PREFIX: `~/.local/bin/tuxgt`, `~/.local/bin/tuxgt-launcher`, `~/.local/share/applications/tuxgt.desktop` (Categories=`Game;`), `~/.local/share/icons/hicolor/<size>/apps/tuxgt.png` (9), `~/.config/tuxgt.conf` (`TUXGT_DATA=<abs PREFIX>`), `~/.config/protonfixes/localfixes/` (file-level compose of `default.py` / `tuxgt.py`; never a dir symlink; wraps a foreign `default.py` to `_tuxgt_wrapped_default.py`). No per-game `.desktop`. To relocate: `mv <PREFIX> <NEW> && <NEW>/bin/tuxgt install`.

### Protonfixes hook (copies, not symlinks)

`tuxgt install` (`install_proton_hook`) writes **files**. Settings → General → TuxGT Install runs the same `install --yes` (and `uninstall --yes`) against `data_dir()`. Intended files are overwritten from the baked copy every install (including modified-ours hook files, per the marker rule below). Stdout lists PREFIX layout, PATH/desktop/conf, `hooks\t$PREFIX/share/protonfixes/`, `icons\t<ok>/<total>` for the 9 themed icons, and one `hook\t<path>` per written `localfixes` file (native, plus Heroic Flatpak when that tree exists), plus `overlaid\t<src> -> <prefix> (<n> files)` on a reinstall overlay.

| Path | Writer | Launch |
|---|---|---|
| `$PREFIX/share/protonfixes/*` | `tuxgt install` (baked `include_str` in the **binary**) and `make deploy` (**repo** `src/protonfixes-hook/`) | `tuxgt_apply.py` is the correlator + session apply |
| `~/.config/protonfixes/localfixes/tuxgt.py` | `tuxgt install` only | Shim: `sys.path` ← prefix `share/protonfixes`, `from tuxgt_apply import apply` |
| `localfixes/default.py` | `tuxgt install` only | Protonfixes entry. Marker `tuxgt-proton-hook v1` → overwrite from baked copy. Else wrap: foreign file → `_tuxgt_wrapped_default.py` once |

`make deploy` then `tuxgt install` can revert prefix `share/protonfixes/` to the last Cargo bake. Day-to-day `tuxgt_apply.py` work: `make deploy` only. Stub/shim refresh: `tuxgt install` after a rebuild.

### Host inventory + verify + reconcile

Last successful install is a TOML inventory at `$PREFIX/config/host-install.toml`
(`HostInstallInventory`: installed host `paths` with symlink targets / file
sha256, plus `wrapped` — the `localfixes` dirs where a foreign `default.py`
was renamed to `_tuxgt_wrapped_default.py`, so uninstall can restore it).
The intended set is computed from **this binary** every time (baked hook
text, conf text, symlink targets); verify compares intended vs disk, not
inventory vs disk. Missing inventory → extras unknown; every intended path is
still verified. Never tracked: `localfixes/__init__.py` (created empty if
absent; not ours to remove), PREFIX internals, `environment.d` (install
inventory still does not own it; retire of `90-tuxgt.conf` is `migrate_env`,
not install).

Each `tuxgt install` reconciles: build the intended manifest, load the
previous inventory, remove previous-inventory paths no longer intended **only
while we still own them** (our symlink target, old-inventory file sha256, or
the `tuxgt-proton-hook v1` marker), then write the intended set and save the
new inventory. A stale path the user replaced (bytes no longer match the old
inventory, no marker) is left in place and reported (`InstallReport`:
`stale_removed` / `stale_skipped`). PREFIX internals, `games/`, and `config/`
are never deleted (the inventory file itself is rewritten, not removed). No
previous inventory (legacy install) → skip the stale pass; write and save.

| Disk | Result |
|---|---|
| missing | `missing` |
| symlink, target matches | `ok` |
| symlink, target differs / dangling | `wrong-target` |
| file, sha256 matches intended | `ok` |
| file, sha256 differs | `modified` |
| kind mismatch (file vs symlink) | `wrong-kind` |

`localfixes/default.py` has two intended contents: the plain hook copy, or
the wrap shim when a foreign default is (or was) present.

### Verify at start + `install --check`

Every GUI start calls `verify_host_install` for `data_dir()` / `$HOME` once
and stores the report on `Shell`. It never blocks the window and never toasts.
Settings → General → TuxGT Install shows the report.

`tuxgt install --check` prints one line per non-`ok` intended path
(`path\tstate`: `missing`, `modified`, `wrong-target`, `wrong-kind`), except the 9 themed icons collapse to one `icons\t<ok>/9\tmissing|modified` line, then
always `ok\t<ok>/<total>`, and exits 0 iff every intended path is `ok`
(anything else exits 1). It never prompts and never writes. `--prefix`
checks that prefix instead of the boot `data_dir()`.

### Host uninstall

`tuxgt uninstall` removes the tracked host files of the last successful
install and leaves PREFIX alone. It loads `$PREFIX/config/host-install.toml`
(prefix from `data_dir()`); a missing inventory errors telling the user to
run `tuxgt install` once — the delete set is never guessed from the intended
manifest. Each recorded path goes through the ownership rule (our symlink
target, recorded file sha256, or the `tuxgt-proton-hook v1` marker); a path
the user replaced is left in place and reported as `skipped`. Wrapped foreign
`default.py` files are restored via `uninstall_proton_hook` on each recorded
`localfixes` tree (native + Heroic), and the inventory file itself is removed
last. PREFIX, sqlite, `games/`, `mods/`, `downloads/`, and `environment.d`
are never deleted (only files and symlinks are ever removed; directories
stay). `--yes` skips the tty confirm; without a tty and without `--yes` it
refuses. Output is one `removed\t<path>` / `skipped\t<path>` line per path
(including `localfixes/tuxgt.py` and `default.py`), except the 9 themed icons collapse to one `removed\ticons\t<ok>/9` / `skipped\ticons\t<ok>/9` line.

### Self-update (`tuxgt update`, MAP `app.self-update`)

MAP `app.self-update` Exists.

Own version (`CARGO_PKG_VERSION`) vs the latest non-prerelease
`waddlr/tuxgt` GitHub release (`tuxgt.tar.gz` asset, same tarball
`make release` drafts). `tuxgt update --check` prints
`current`/`latest`/`status` and writes nothing; `tuxgt update` confirms
on a tty (`--yes` skips; no tty without `--yes` refuses) and applies.
Tags compare as `vX.Y.Z` with optional `-beta.N` (final beats beta);
an unparseable tag or a release without the asset is `unknown`, never
an error that blocks the app. The update path never opens the DB.

Apply order: fetch the asset into `downloads/` (forced re-fetch) →
unpack to a dot-prefixed dir under `downloads/` (removed afterwards)
→ refuse when the tree has no `bin/tuxgt`, holds a symlink, or holds a
path outside the owned roots (`bin/`, `lib/`, `share/`,
`mods/official/`) → overlay every owned file into the live prefix
(tmp + rename per file; `bin/*` and `lib/*.so` get `755`, the rest
`644`, mirroring `prepare`; `share/applications/tuxgt.desktop` keeps
the deploy rule — written only when missing) → re-hash every overlaid
file against the new manifest → drop previous-owned files missing from
the new set (a dropped `mods/official/<id>.toml` also drops
`mods/official/<id>/`, deploy parity) → save
`$PREFIX/config/prefix-manifest.toml` (`tag` + owned `path`/`sha256`
rows) → re-exec host refresh as the NEW binary
(`$PREFIX/bin/tuxgt install --prefix $PREFIX --yes`, pinned so the live
prefix never moves to `~/tuxgt`), so baked hook text comes from the new
build → drop the spent download entry. No previous manifest
(legacy install) skips the stale pass, like the host inventory.
`games/`, `downloads/` (at rest), `config/` (except the manifest
itself), `mods/user/`, registries, and kept official payloads are
never touched. A failed host refresh errors loudly (the prefix is
already new; re-run `tuxgt install`). GUI apply drops the `app:<tag>`
Attention card as soon as Update is clicked (action taken), opens a
Live card for fetch progress with Cancel (About morphs to Cancel too),
and restores `Available` plus the Attention card on cancel or failure.
A new available tag also cards. Cancel aborts the fetch only (keeps
`.part`); once overlay has started, cancel is ignored. CLI has no
cancel.

## Monorepo (real paths)

One git root. Duplication across stacks is fine.

| Component | Path | Docs |
|---|---|---|
| launcher | `src/launcher/` | `docs/dev/launcher/` |
| addons (dropped) | — | no spec; `IncludeFile` is in `docs/dev/launcher/overview.md` |
| app | `src/tuxgt/` | `docs/dev/app/` |
| protonfixes hook | `src/protonfixes-hook/` | this file |

`external/` is read-only reference, never compiled into any component. `make prepare` copies `build/libtuxgt-launcher.so` and `src/tuxgt/target/release/tuxgt` (plus the launcher script, official TOMLs under `mods/official/`, templates, and hook scripts). It does not copy `external/`, or `mods/contrib/`. NVStreamline was dropped on reset (no `make deploy-nvstreamline`).
