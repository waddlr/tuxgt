# Build

This page is for developers and contributors who build TuxGT from source. End users should follow [Install](install.md) — they download the prebuilt tarball and never run these commands.

All commands below are verified against the `Makefile` in the repo root. Run them from the repo root.

## Prerequisites

- `gcc` and `make` — for the preload library (`src/launcher/*.c` → `build/libtuxgt-launcher.so`, built as `gcc -shared -fPIC -O2 -fvisibility=hidden -o … -ldl -lcrypto`). The resulting `.so` needs `libcrypto.so.3` at runtime (same as the shipped tarball).
- **Rust toolchain** with `cargo` — stable is fine (`rustc` + `cargo`). TuxGT is a Cargo workspace at `src/tuxgt` (edition 2021, members `tuxgt-core` and `tuxgt-app` — the binary crate is named `tuxgt`). No extra `rustup` components are required.
- `tar` and `gzip` — for `make package` (`tar -czf`).

You do not need a Proton install, Heroic install, or any store account to build.

## Targets

### `make build` (`TYPE=dev|trace|release`, default `dev`)

```sh
make build                  # dev: fast incremental debug build
make build TYPE=trace       # release opts, unstripped + line tables
make build TYPE=release     # what ships: stripped, `lto = "thin"`
```

| `TYPE` | Cargo invocation | Binary |
|---|---|---|
| `dev` (default) | `cargo build -p tuxgt` | `src/tuxgt/target/debug/tuxgt` |
| `trace` | `cargo build --profile release-trace -p tuxgt` | `src/tuxgt/target/release-trace/tuxgt` |
| `release` | `cargo build --release -p tuxgt` | `src/tuxgt/target/release/tuxgt` |

Day-to-day hacking is `make build` / `make deploy` (both default `dev`). `prepare`/`package`/`deploy` inherit `TYPE`, so `make deploy TYPE=release` stages and overlays the release binary.

### `make prepare`

```sh
make prepare
```

Depends on `preload` and `build`. Fills `build/pkg/tuxgt` from repo files and built artifacts:

- `build/libtuxgt-launcher.so` → `$PKG/lib/libtuxgt-launcher.so`
- `src/tuxgt/target/<debug|release-trace|release>/tuxgt` (per `TYPE`) → `$PKG/bin/tuxgt`
- `src/launcher/tuxgt-launcher` → `$PKG/bin/tuxgt-launcher`
- `src/protonfixes-hook/{tuxgt_apply.py,default.py,default_wrap.py,tuxgt.py}` → `$PKG/share/protonfixes/`
- `src/tuxgt/tuxgt-app/assets/icons/hicolor/<16,24,32,48,64,128,256,512,1024>/apps/tuxgt.png` → `$PKG/share/icons/...`
- `mods/official/*.toml` → `$PKG/mods/official/`
- `mods/templates/*.toml` → `$PKG/share/templates/`
- Writes `$PKG/share/applications/tuxgt.desktop` (`[Desktop Entry]` `Name=TuxGT` `Exec=tuxgt` `Icon=tuxgt` `Categories=Game;`)

and creates empty `$PKG/games/`, `$PKG/downloads/`, `$PKG/config/` for layout. No ReShade payloads, no NVStreamline binaries.

Inspect the tree after: `tree build/pkg/tuxgt` or `ls -R build/pkg/tuxgt`.

### `make package`

```sh
make package
```

Depends on `prepare` (so building the package always rebuilds the product tree first). What it runs:

```sh
rm -rf dist
mkdir -p dist
tar -czf $(CURDIR)/dist/tuxgt.tar.gz -C build/pkg tuxgt
```

Result: `dist/tuxgt.tar.gz` — unpacks to a single top-level `tuxgt/` directory with the layout shown in [Install](install.md). Only `make package TYPE=release` (or `make release`, which forces it) produces the shippable tarball; a default `make package` bundles the dev binary for a quick layout check. Extract to test:

```sh
tar -tzf dist/tuxgt.tar.gz | head
```

### `make deploy`

```sh
make deploy
```

Live-prefix overlay for day-to-day hacking. Depends on `prepare`, then copies the prepared tree into the live prefix `$$pfx` taken from `~/.config/tuxgt.conf` (`TUXGT_DATA` — the same conf written by `tuxgt install`). If the conf is missing, empty, or `TUXGT_DATA=/`, it errors with "run `tuxgt install` first".

What it syncs (as the Makefile's `CONF_PFX` loop does):

- `bin/tuxgt`, `bin/tuxgt-launcher`, `lib/libtuxgt-launcher.so`, `share/protonfixes/*`, `share/templates/*.toml`, `share/icons/...` — always.
- `share/applications/tuxgt.desktop` — only if missing (does not overwrite your desktop file).
- `mods/official/*.toml` — write/update each shipped recipe; delete any `$PREFIX/mods/official/*.toml` that left the package together with its payload dir `$PREFIX/mods/official/<id>/` where it exists.

What it never touches: `games/`, `downloads/`, `config/` (so `config/tuxgt.sqlite` and your index stay), `mods/user/`, any registry under `mods/<registry>/`, or payload dirs of still-shipped officials. Host files under `~/.local/` and `~/.config/` are not written by `deploy` — that is `tuxgt install`'s job.

After `deploy`, run `tuxgt install --check` if you want to verify host symlinks/icons.

### Other targets

- `make preload` — just `build/libtuxgt-launcher.so`.
- `make clean` — removes `build/` contents except `build/pfx-*`, plus `dist/` and `src/tuxgt/target` (`cargo clean`); `make clean-full` also drops `build/pfx-*`.
- `make all` — same as `make preload`.

No `make install` exists — use `tuxgt/bin/tuxgt install` after unpacking, or `make deploy` for the live prefix.

### `make release`

Maintainer-only: tags and drafts a GitHub release with the user tarball — nothing publishes automatically; a human reviews and publishes the draft on GitHub. Always builds `TYPE=release` first regardless of any `TYPE=` on the command line — only true release builds ship.

```sh
make release BUMP=minor        # bump, tag, draft
make release                   # draft current version (fails if already out, draft included)
make release BUMP=patch DRY_RUN=1   # print every step, change nothing
make release BUMP=beta         # prerelease line: minor bump once, then beta counter (vX.Y.Z-beta.N)
make release BUMP=promote-beta # graduate the in-flight beta to its final
```

Requires a clean tree on `master`, `gh` installed and authed (`pacman -S github-cli`, `gh auth login`), and the branch pushed. With `BUMP`, the workspace version in `src/tuxgt/Cargo.toml` is bumped, `Cargo.lock` re-synced offline, committed as `Release <tag>`, and tagged. `major`/`minor` are refused while a beta is in flight; `patch` on a beta base hotfixes the last final instead. Without `BUMP`, the current version drafts unless that tag already has a release or draft on GitHub. Either way the tag and `master` are pushed, then `gh release create --draft` uploads `dist/tuxgt.tar.gz` with notes generated from the commit shortlog (since the last final for finals, since the previous tag for betas). Beta tags always draft with `--prerelease`, finals never do — no manual flag, so a beta can never hijack `releases/latest/` (which ignores drafts and prereleases).

## Repo layout

```
Makefile                       # all targets above
LICENSE.md                     # MIT
src/launcher/                  # C preload + wrapper (libtuxgt-launcher.so, tuxgt-launcher)
src/tuxgt/tuxgt-app/           # GUI + CLI binary (crate name `tuxgt`, assets under assets/)
src/tuxgt/tuxgt-core/          # core library (plugins, registry, scan, mods, launch, sqlx index)
src/tuxgt/tools/gui-session    # headless sway lane for screenshots/tests (not the product)
src/protonfixes-hook/          # tuxgt_apply.py, tuxgt.py, default.py, default_wrap.py
mods/official/                 # shipped recipes (reshade, optiscaler, d3dcompiler-47)
mods/templates/                # Add Pack templates (8 files, shipped at share/templates/)
build/                         # build artifacts (libtuxgt-launcher.so)
build/pkg/tuxgt/               # staged product tree (from `prepare`)
dist/tuxgt.tar.gz              # user tarball (from `package`)
docs/build.md                  # this file
docs/install.md                # end-user counterpart (tarball layout, update, uninstall)
```

## Clean checkout smoke test

```sh
git clone https://github.com/waddlr/tuxgt.git
cd tuxgt
make package
tar -tzf dist/tuxgt.tar.gz | head -20
./build/pkg/tuxgt/bin/tuxgt --help
```
