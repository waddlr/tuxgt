# Mods

ReShade alone is not a product. Graph, Mods, Instances (FileManifests), and loader prewire live in core.

Vocabulary: a **Plugin** is an interface; a **Mod** is a definition (recipe TOML); an **Instance** is the per-game runtime of a Mod (FileManifest + staging). Settings → Mods lists Mods; the Game Mods tab lists Instances.

## Package kinds

| Package type | What it is | Depends on | Example |
|---|---|---|---|
| `reshade` | stock-named `ReShade64.dll` (a proxy-named ReShade never hooks its self-named API) | — | official ReShade 6.8.x |
| `optiscaler` | proxy-slot DLL (`dxgi`) + `OptiScaler.ini` + companion tree | — | official or a fork Mod |
| `reshade_addon` | `.addon64` **plus supporting files** | a ReShade Mod | TuxGT NR = `tuxgt-nr.addon64` (+ supporting files; NV/DLSS via `custom`) |
| `custom` | user-supplied DLL + slot pick, no quirks | — | NV/DLSS/nvngx zips (Custom Mod) |

Reserved, not implemented: `env_tool`. `effect` (`.fx`) and `texture` ship with dest roots (`reshade-shaders/Shaders|Textures`); RenoDX/Luma ship as per-game `reshade_addon` recipes. vkBasalt is not planned.

## Dependency + conflict

Core, not per-plugin:

- `requires: [{ type: reshade }]` — missing ReShade → prompt to install default instance. Hard block if declined.
- `conflicts: [{ slot: dxgi }]` — two injectors wanting the same proxy name.
- Soft warnings: anti-cheat, foreign DLL, 32-bit game, API mismatch.

Show the graph on the Mods tab. Never silently enable a dependency. Never silently pick a DLL name (dest slot).

## Mods

The unit of configuration is a **Mod**, not a singleton per type.

```
mod {
  id: "optiscaler" | "optiscaler-xyz"
  type: optiscaler
  label: "OptiScaler"
  source: github { owner, repo, asset_glob } | local { path } | manual_url
  plans_allowed: [install, preload]
  payload: [[{ arch, api, keep[], drop[] }]]
}
```

- `proton_env` (`PROTON_USE_OPTISCALER=1`) is official-only. Shipped OptiScaler recipes do not list it, so a manual install never sets it. A user Mod listing the plan is rejected.
- Official Mods are optional: disable, do not remove (`docs/dev/app/plugin/instances.md`). Disabled Mods are not offered on the Game Mods tab. Existing per-game manifests stay; no cascade uninstall. Safe default is the first **enabled** official + safe plan; if none, missing-require (prompt). The choice is always shown.

Sources: GitHub Release (asset glob + sha256); local/offline directory. Nexus-only packages: open the Nexus page in the browser, user saves into the download/cache folder, then pick as local. No Nexus API.

## FileManifest + harvest

Every enable/install writes a manifest: planned files (source, dest, sha256, adapter, mod id, per-dest keep), generated globs, backups of overwritten game-dir files, instance enabled flag. The loader keeps its own runtime manifest (`docs/dev/launcher/overview.md`); the app FileManifest records install-time intent and harvests generated files.

Per-dest keep (`[[files]].enabled`, missing = true) is **this game only**: omit optional companions from staging / install copies and from `LoadDLL` lists; nested non-DLL dests stay covered by their `IncludeFile` tree line. Required dests cannot be omitted (`docs/dev/app/plugin/download.md`). An applicable `.dll` can also switch LoadDLL and IncludeFile for this game (`[[files]].load`); that does not change keep or the slot. Not a raw `tuxgt-launcher.ini` editor.

Harvest after a TuxGT Play, or on next scan. Uninstall deletes tracked dests, restores backups, leaves foreign files.

Per-game: **list those files** as keep rows on the installed Mods card (not a full viewer, not a global dump). Buttons: open install dir, prefix dir, TuxGT per-game dir.

## Prewire the loader

`GameInfo` exists before install:

1. Detector fills exe stem, `Type` (`dx9_64` / `dx10_64` / `dx11_64` / `dx12_64` — also `dx9_32` / `dx10_32` / `dx11_32` / `dx12_32` when 32-bit), bitness, API.
2. Enabling an injector writes `[Stem]` immediately: `Type` (informational), `LoadDLL` entries, `IncludeFile` data.
3. Per-game runtime dir is created and files staged **before** first launch.
4. First Play is a normal launch.

Loader opt-in stays: no section ⇒ no-op.

## First-party packages

**ReShade + addon packages.** Runtime: download at runtime, stock filenames in depot trees (`ReShade/<ver>/`, …). Do not redistribute in git. Do not put ReShade, NVStreamline, or first-party addons in `make package` / `make deploy` (`docs/dev/app/core/install.md`). Addon package: zip/dir with `.addon64` + extra files mapped to `IncludeFile` vs `LoadDLL`. Multiple ReShade Mods = depot trees; dest slot picked per game. First-party addons and NVStreamline were dropped on reset — no local-test Make copies, no catalog Mods. Official Mods ship as packaged TOML (`mods/official/*.toml` in the repo, `$PREFIX/mods/official/*.toml` live) — no hardcoded recipe literals in the binary (`docs/dev/app/plugin/instances.md`).

**OptiScaler.** Official Mod downloads GitHub `optiscaler/OptiScaler` latest `OptiScaler_*.7z` (same acquire path as other github recipes). 64-bit only. Preload dest for `OptiScaler.dll` is `dxgi.dll` (type slot, proven in-game). Forks / the custom v3 tree are a **different Mod id** created from a local package (`docs/dev/app/plugin/instances.md` from-package): scan files, user tweaks keep/dest, extra files default kept, then `instance install` writes them into that game's FileManifest. They do not shadow `optiscaler`. Absolute `OptiDllPath` / per-dll ini rewrite is `launch.staging-rewrites` (Later); official 0.9.x companions stage next to the `dxgi` dest. Depot ini is the source of truth when that rewrite exists (staged copy overwritten every launch). Plans: `docs/dev/app/core/launch.md`.
