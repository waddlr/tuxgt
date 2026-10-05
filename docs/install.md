# Install

> Building from source? See [Build](build.md) — end users do not need it.

TuxGT ships as a self-contained tarball built with `make package`. You download it, unpack it, and run the launcher script inside it once. No `sudo`, no Flatpak, no system package.

## Quick install

```sh
curl -fsSL https://raw.githubusercontent.com/waddlr/tuxgt/master/install.sh | bash
```

That prints a short welcome. A first install asks the directory (default `~/tuxgt`). An existing install (`TUXGT_DATA` with `bin/tuxgt`, or `~/tuxgt`) keeps that prefix and does not ask to move it. It then compares `bin/tuxgt` to the latest GitHub release (`vX.Y.Z`, optional `-beta.N`, final beats beta). Already current or newer skips the tarball; older (or unreadable) asks before downloading `tuxgt.tar.gz` with a progress bar (needs `curl` or `wget`), unpacks it onto that filesystem, then runs the packaged `tuxgt install --prefix … --yes`. Set `TUXGT_PREFIX` to skip the directory prompt. At the end it offers to launch the app. To install by hand instead, follow Download + Install in three commands below.

## Prerequisites

You do not need Rust or `gcc` to run the tarball — it already contains the built app and launcher. You do need:

- `tar` and `gzip` to unpack the archive (the tarball is created with `tar -czf`).
- A 64-bit Linux desktop — TuxGT is a desktop app (KDE, GNOME, etc. both work).
- Games must be DirectX 9, 10, 11, or 12, 32- or 64-bit — Vulkan and OpenGL are not supported.
- The `libcrypto.so.3` library for the preload helper (`build/libtuxgt-launcher.so` is linked with `-lcrypto`). Most distros already ship it — if `ldd ~/tuxgt/lib/libtuxgt-launcher.so` reports it missing, install your distro's OpenSSL package (Arch: `openssl`, Debian/Ubuntu: `libssl3`, Fedora: `openssl-libs`).

No other build tools are needed.

## Download

Download `tuxgt.tar.gz` from the GitHub Releases page:

- Releases: `https://github.com/waddlr/tuxgt/releases`
- Asset: `tuxgt.tar.gz`

No release exists yet in this repo history — this link will work once the first release is published. If you are building locally, `make package` creates the same layout at `dist/tuxgt.tar.gz` (see [Build](build.md)).

## Install in three commands

Pick a download folder (for example `~/Downloads`), then:

```sh
tar -xzf tuxgt.tar.gz
./tuxgt/bin/tuxgt install
tuxgt
```

What those do:

1. `tar -xzf tuxgt.tar.gz` — extracts the directory `tuxgt/` next to the archive. This mirrors how the tarball is built (`tar -czf dist/tuxgt.tar.gz -C build/pkg tuxgt` in the Makefile), so the top level is always `tuxgt/`.
2. `./tuxgt/bin/tuxgt install` — must be run from the unpacked tree (`…/bin/tuxgt` is required — running a copy elsewhere errors with "run the packaged binary"). Interactive when a terminal is present: it prompts `Install prefix [/home/you/tuxgt]:` (the absolute default) and uses `~/tuxgt` if you press Enter. Use `--prefix ~/my-tuxgt` to skip the prompt with a different location, or `--yes` to accept the default without prompting. It moves or keeps the tree at the chosen prefix, then writes host files and prints the chosen `PREFIX`, `bin`, `mods`, and `config` lines plus `to relocate: mv <PREFIX> <NEW> && <NEW>/bin/tuxgt install`.
3. `tuxgt` — after install, `~/tuxgt/bin/tuxgt` is linked to `~/.local/bin/tuxgt`, so `tuxgt` (or `tuxgt gui`) opens the window. You can also start it from your app menu ("TuxGT" under Games).

### What `install` writes

On success it creates/updates only these host files (the game data stays under the prefix you chose):

- `~/.local/bin/tuxgt` → `$PREFIX/bin/tuxgt`
- `~/.local/bin/tuxgt-launcher` → `$PREFIX/bin/tuxgt-launcher`
- `~/.local/share/applications/tuxgt.desktop` (Categories `Game;`)
- `~/.local/share/icons/hicolor/16x16` through `1024x1024` copied from `$PREFIX/share/icons/hicolor/*/apps/tuxgt.png` (9 sizes: 16, 24, 32, 48, 64, 128, 256, 512, 1024)
- `~/.config/tuxgt.conf` containing `TUXGT_DATA=<absolute PREFIX>`
- `~/.config/protonfixes/localfixes/tuxgt.py` and `localfixes/default.py` (plus a Heroic Flatpak copy when that tree exists) — the protonfixes hook

It also writes the prefix tree itself (see layout below) and saves an inventory at `$PREFIX/config/host-install.toml` for later verification and clean uninstall. Re-running install reconciles stale host files: files you replaced yourself are left alone and reported as `skipped`.

Check without changing anything:

```sh
tuxgt install --check
```

That prints one line per non-`ok` host path (`missing`, `modified`, `wrong-target`, `wrong-kind`, or a collapsed `icons  n/9` line) then `ok  <ok>/<total>`, and exits 0 only when all host files are ok. `--prefix` checks a different prefix than the booted one.

## Tarball layout

Unpacking `tuxgt.tar.gz` gives `tuxgt/` with exactly what `make prepare` puts into `build/pkg/tuxgt` (no ReShade payloads, no addons):

```
tuxgt/
  bin/tuxgt                         # the app (built with `cargo build --release -p tuxgt` → src/tuxgt/target/release/tuxgt)
  bin/tuxgt-launcher                # helper script (src/launcher/tuxgt-launcher)
  lib/libtuxgt-launcher.so          # preload helper (built with `gcc -shared -fPIC -O2` + -ldl -lcrypto)
  mods/official/*.toml                  # 9 recipes: optiscaler, optiscaler-y4my4m-v4, reshade,
                                        # d3dcompiler-47, nvidia-streamline, nvngx-dlssnr,
                                        # nvngx-dlssnr-proxy, renodx-dlss, deep-fried-chicken-64bit
  share/applications/tuxgt.desktop
  share/icons/hicolor/<16..1024>/apps/tuxgt.png   # 9 sizes
  share/templates/*.toml             # 8 templates (see Mods)
  share/protonfixes/tuxgt_apply.py
  share/protonfixes/default.py
  share/protonfixes/default_wrap.py
  share/protonfixes/tuxgt.py
  config/                            # empty at ship — holds tuxgt.sqlite after first run
  games/                             # empty
  downloads/                         # empty
```

Payload DLLs for OptiScaler/ReShade and NVIDIA Streamline are **not** in the tarball — the downloaded ones are fetched at runtime, the provided ones you supply yourself (see [Mods](mods.md)).

## Updating

The app checks the [Releases](https://github.com/waddlr/tuxgt/releases) page for you: when a new version is out you get a notice with an **Update** button, and **Settings → General** grows an **Update to …** button next to **Repo**. Starting an update dismisses that notice and shows a progress card you can **Cancel**. In a terminal the same thing is one command (it asks for confirmation unless you pass `--yes`; `--check` only prints the current and latest versions):

```sh
tuxgt update
```

This refreshes `bin/`, `lib/`, `share/`, and `mods/official/` (tracked, so files a release drops are removed again) while leaving `games/`, `downloads/`, `config/`, and `mods/user/` alone, then refreshes the host files from the new build. Restart TuxGT after updating.

To update by hand instead, download the newer `tuxgt.tar.gz` from [Releases](https://github.com/waddlr/tuxgt/releases), unpack it somewhere OUTSIDE your prefix (for example `~/Downloads`), then run the new tree's installer against the prefix:

```sh
cd ~/Downloads
tar -xzf tuxgt.tar.gz
./tuxgt/bin/tuxgt install --prefix ~/tuxgt --yes
```

(If your prefix is not `~/tuxgt`, use that path. The installer overlays the new program files atomically — dropped official recipes are removed — while leaving `games/`, `downloads/`, `config/`, and `mods/user/` alone.)

To relocate instead of updating in place:

```sh
mv ~/tuxgt ~/tuxgt-new && ~/tuxgt-new/bin/tuxgt install
```

## Uninstall

Removes the host files tracked in `$PREFIX/config/host-install.toml` and leaves the prefix data alone:

```sh
tuxgt uninstall
```

It prompts for confirmation when a terminal is present; add `--yes` to skip it (`tuxgt uninstall --yes`). Missing inventory errors with "no host inventory at …: run `tuxgt install` once" — nothing is guessed. Each path is removed only while we still own it (symlink target, file sha256, or the `tuxgt-proton-hook v1` marker); files you edited yourself are left and reported as `skipped` (icons collapse to one `removed  icons  n/9` / `skipped  icons  n/9` line). Wrapped foreign `default.py` files are restored.

The prefix itself (`~/tuxgt` by default — `bin/`, `lib/`, `mods/`, `games/`, `downloads/`, `config/tuxgt.sqlite`, `logs/`, `share/`) is not deleted. To fully remove data after uninstall:

```sh
rm -rf ~/tuxgt
# then also, if you want, remove the symlinks the uninstall left due to ownership: check `tuxgt install --check`
```

No `sudo` and no `make install` — the project has no such target.

## Help and next steps

- First run walkthrough: [Quickstart](quickstart.md)
- Mods and templates: [Mods](mods.md)
- Something wrong: [Troubleshooting](troubleshooting.md)
- Want to build: [Build](build.md) ("building from source")
