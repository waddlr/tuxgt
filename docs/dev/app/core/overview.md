# App product rules

The C preload (`docs/dev/launcher/overview.md`) stays. This is the desktop + CLI layer on top of it, not a rewrite of `libtuxgt-launcher.so`.

GUI crate: **gpui-kit**. Until this file records exactly one of `iced` or `gpui-kit`, no GUI exec may run. Recorded decision: gpui-kit.

If a plugin contract disagrees with this folder, **this folder wins**.

## Positioning

Linux-native, Proton/Wine-aware manager for **injector / runtime mods**, plugin-structured, with a card library and two launch adapters (preload default, install when files must sit next to the exe).

Not a store, not Amethyst (Nexus/FOMOD/VFS/load order), not a Proton installer, not a Windows app with a Linux port.

**Linux wedge vs RHI:** preload that never writes the game install; ProtonDB badge + site link; prefix/Proton/umu/SLR; compose wrappers; OptiScaler via a user-visible plan (env vs install-proxy vs preload — preload via the `dxgi` slot proven in-game); Flatpak-aware *discovery of Steam/Heroic* (TuxGT itself is not a Flatpak).

| Project | Steal | Don't |
|---|---|---|
| **RHI** | Cards + left-nav, API/engine detect + override, component rows, DLL-slot conflicts with a safe default, per-game overrides, foreign-DLL protection, version channels, custom-build folders | Game-dir copy as the only path; NVIDIA driver writes; HDR auto-toggle; MOTD |
| **Amethyst** | Steam+Heroic detection, restore of runtime-generated files | MO2 UI, collections, 100 game pipelines |
| **NOMM** | Recipe files so non-programmers add a source; Steam cache art; no ads/telemetry/accounts | Zip-extract as the product |

## Architecture

One binary. No-args or `tuxgt gui` opens the GUI. Subcommands are the same operations.

```
tuxgt
tuxgt scan
tuxgt games list [--manager steam] [--store gog]
tuxgt game show steam::814380
tuxgt launch steam::814380              # Play: no store mutation required
tuxgt launch --apply steam::814380      # persist wrapper into Steam/Heroic, then play
tuxgt doctor steam::814380
tuxgt plugins list
tuxgt env list|set|unset|enable|disable
tuxgt env global list|set|unset|enable|disable
tuxgt mods list|add|add-from|rescan|remove|enable|disable
tuxgt instance install|uninstall|enable|disable|files|env|status|slot <game> <mod>
```

**Core is not a plugin.** The C launcher stays C. The app **prewires** the per-game ini from `GameInfo` at enable-time (exe stem, `Type` informational, explicit `LoadDLL` depot paths + dest slots, `IncludeFile` data), not on first PE thread.

A **plugin is a bundle**: many capabilities and many user-facing options.

**Plugin / Mod / Instance.** A **Plugin** is an interface (GameProvider, MetadataSource, EnvKnob, Wrapper, Mod) surfaced as a PluginHost row. A **Mod** is a definition: where to get content, how and where it installs (LoadDLL vs IncludeFile, remap), **Requires** (other Mods), **Provides**, `[env]`, `[[payload]]` gates. An **Instance** is the per-game runtime of a Mod (FileManifest + staging). Settings → Mods lists Mods; the Game Mods tab lists Instances. Install picks a Mod and creates an Instance — there is no per-game file drop.

## Plugin capabilities

| Capability | Job | First-party |
|---|---|---|
| `GameProvider` | Scan, artwork, current launch config, optional Apply | Steam, Heroic, Manual |
| `Provides` | What kind a Mod provides (how a package is installed) | `reshade`, `optiscaler`, `reshade_addon`, `custom`, `effect`, `texture` (schema reserves `env_tool` — unimplemented) |
| `Mod` | Named definition: content source, dests, Requires, Provides, env, payload gates | Official ReShade, official OptiScaler, plus user-added |
| `Instance` | Per-game runtime of a Mod (FileManifest + staging) | one per (game, Mod) |
| `LaunchAdapter` | Preload / install / env-plan → `LaunchSpec` | preload, install; env is a plan on the injector, not a third adapter |
| `EnvKnob` | Named env-controllable setting | first-party Proton/GPU knobs |
| `Detector` | Bitness, API, engine, native vs PE | PE/ELF (core utility, not a plugin row) |
| `MetadataSource` | Badges / art / links | ProtonDB, SteamGridDB, AreWeAntiCheatYet |

Contribution layers: (1) recipe TOML, (2) in-tree first-party, (3) external ABI later — do not freeze ABI. Engineering bind: `docs/dev/app/plugin/`.

## Never first-party

Lutris, Bottles, Faugus; Nexus API (browser + “put the file here” only); FOMOD/VFS/load order; Proton version manager; Windows NVIDIA driver profiles; Flatpak or AppImage of TuxGT; launching Heroic through Flatpak; library folder watch; first-class vkBasalt; desktop autostart; GNOME-specific UI; Steam Deck Gaming Mode / big-picture; Slint; rewriting the C launcher; redistributing ReShade/OptiScaler in git; system-wide install; iced in the product binary; Qt; Tauri/WebView. Flatpak discovery of Steam and Heroic stays. (Self-update from GitHub releases is first-party and not built on this tree yet: `install.md` Self-update, MAP `app.self-update`.)

## Stack (locked)

**Language:** Rust in `src/tuxgt/` (`tuxgt-core` + `tuxgt` bin). C launcher stays C.

```
src/tuxgt/
  tuxgt-core/     # no GUI: plugins, registry, sqlx index, downloader, launch
  tuxgt-app/      # clap CLI + gpui-kit GUI (crate name tuxgt)
```

**GUI:** gpui-kit 0.6 (Apache-2.0; `gpui-pre-*` from crates.io, never zed git). iced 0.14 is a documented alternative, not the product crate. Out: Slint, libcosmic, gpuix, GTK, Tauri/WebView, egui, Qt/Kirigami.

Product gpui-kit: `default-features = false`, `features = ["component"]`; do not enable `inspector` or `tree-sitter-*`. Skip extra work while unfocused, occluded, minimized, or while a launched game is fullscreen. No timers/animations unless a visible progress widget needs them. Timer exception: the 2h catalog update poll — one one-shot re-armed per run, skipped while unusable, catch-up on focus return; CLI never polls.

**Store:** files are source of truth. **SQLite via `sqlx`** (tokio + rustls + bundled sqlite + FTS5) is the index.

**HTTP / hash:** `reqwest` + rustls (verify on) + Range resume + `sha2`. No OpenSSL in the app binary.

**Launcher SHA-256:** preload `.so` `NEEDED` is libc + `libcrypto.so.3`. App stays rustls/`sha2`.

**Keyring:** `keyring` crate (Plasma `ksecretd`). Never plain-text in config.

**Licenses:** app MIT. `cargo-deny` allows MIT/Apache/BSD/0BSD/ISC/MPL/Unicode/Zlib/OFL/**LGPL**/CC0/bzip2. Denies GPL-3/AGPL/GPL-2. **Never Qt.**

**Standard crates:** `clap`, `tokio`, `serde`+`toml`+`serde_json`, `reqwest`, `sha2`, `sqlx`, `goblin`, `tracing`, `thiserror`, `image`, `keyring`, `open`. Niche: `steamlocate`, `keyvalues-parser`. `notify` is later.

## Security (non-optional)

HTTPS with certificate-verified TLS. Secrets in the system keyring only. Downloader: TLS + sha256 before install.

## Other files in this folder

| File | Job |
|---|---|
| `identity.md` | Game ids |
| `mods.md` | Package kinds, graph, instances, prewire |
| `launch.md` | Play vs Apply, adapters |
| `install.md` | Userland prefix, real source tree |
