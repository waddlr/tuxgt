## Detector

Types may gain fields later; not an external ABI.

Not in this surface:

| Item | Where |
|---|---|
| Loader ini prewire / `Type=` key | `launch-adapter.md` (`{api}_{bitness}` when both known) |
| Inject / refuse 32-bit | later (32-bit is stored; Play still allows it) |
| GUI force-redetect | `docs/dev/app/gui/game.md` (same confirm + wipe) |
| Override editor CLI (`--set` / `--unset`) | shipped; flags under [CLI](#cli) |
| Apply write of launch options | `apply.md` |
| PluginHost rows for detectors | not v1 (see below) |

### Not a plugin

Detection is a **core utility** the scan / `games add` / `doctor` pipeline calls per indexed game. It is not a `PluginHost` bundle and does not appear in `tuxgt plugins list`.

Internal `Detector` trait + static table (first-party impls: `pe`, `unreal`, `unity`, `re-engine`, `creation`, `blackspace`, `mo2`, `runtime`). Core parses the exe **once** (`goblin` PE or ELF) into `BinaryInfo` and passes it in; impls must not re-read the file. A strict goblin PE parse that fails on Authenticode or version-resource extras retries with certificates/TLS skipped and permissive mode, then a COFF Machine header fallback for bitness only (empty libs). goblin 0.10 `PE.libraries` is the import table (IAT) only; delay-import `DllNameRVA` names are unioned into `BinaryInfo.libs` from the delay-import data directory (IMAGE_DELAYLOAD_DESCRIPTOR, RVA or VA name form). The image is not scanned for ASCII DLL strings. This is the pluggable shape without plugin-row cost (disabling `pe` would blank doctor; engine heuristics are not user-facing bundles). Impls may attach to plugin ids later without changing the trait.

### Pipeline

1. GameProvider upserts identity + **store snapshot** (what the manager already knows).
2. Core builds `DetectInput` from the row (id, install_dir, store exe/prefix/proton/build).
3. If not `--force` and the fingerprint matches, skip detectors **unless** a known exe file's current `parse_binary` disagrees with stored PE-owned bitness/api/extra. Always refresh the store snapshot.
4. Else: engine locators may fill a missing exe; parse binary once; run detectors; merge; write `detected_*` + fingerprint.
5. Effective field = override if set, else detected, else store. Override never clobbered except `--force`.

Fingerprint: sha256 of path + size + mtime for exe (store or last detected), install_dir, and `prefix/version` when that file exists. Not a hash of the whole binary.

### Trait and merge

Sync.

```
DetectInput:
  id: GameId
  install_dir: Option<PathBuf>
  exe: Option<PathBuf>          # store / last detected
  prefix: Option<PathBuf>
  proton: Option<String>
  build: Option<String>

BinaryInfo:                       # one parse
  path, kind: pe | elf
  bitness: 32 | 64
  libs: lowercase names (IAT + delay-import)
  file_version: Option<String>

Detected: exe, platform, bitness, api, engine, prefix, proton, build, exe_version

Detector:
  id() -> &'static str
  detect(input, binary, &mut Detected)
```

Field owners (conflict: owner overwrites; non-owner with a different value → `tracing::warn` and keep current; empty never overwrites):

| Field | Owner |
|---|---|
| bitness, exe_version, platform=`native` | `pe` |
| api, extra_apis | sidecar first (`unreal` / `unity`), then `pe` IAT + delay-import libs |
| engine | first impl that sets it (`unreal`, `unity`, `re-engine`, `creation`, `blackspace`, `mo2`) |
| exe (when missing) | same order, then fallback walk |
| prefix, proton | store snapshot (Steam VDF / Heroic GamesConfig); not copied into `detected_*` |
| platform=`proton`/`wine` | `runtime` |
| build | store snapshot first; PE/engine only if store empty (and only then `detected_build`) |

`api` is the **default**. `extra_apis` is a comma-separated list of other APIs the install also exposes (UE5 DX12 + `-dx11` → `api=dx12`, `extra_apis=dx11`). `dxgi` alone is not an API. UE shipping: ignore `opengl32` unless no D3D/Vulkan is present. `D3D12/D3D12Core.dll` next to the shipping exe ⇒ default `dx12`. No `type` column: loader Type is `"{api}_{bitness}"` when both are `dx9`/`dx10`/`dx11`/`dx12` and `32`/`64`.

Online catalogs (PCGamingWiki, …) are **not** consulted at detect time. See `.agents/docs/plans/detect-catalog.md`.

### Host GPU

Not a per-game `Detected` field. Env Advance split (`docs/dev/app/plugin/env-knob.md`) reads PCI vendors from `/sys/class/drm/card*/device/vendor`: `10de` nvidia, `1002` amd, `8086` intel. Hybrid = the set found. Missing sysfs → unknown (all GPU groups stay in main). No `lspci`, no vulkan.

### Inputs / locators

- Manual: exe is the added file; prefix only if later override.
- Steam: install_dir from scan; shortcut exe when the path exists on disk; `buildid` from appmanifest; `compatdata/<id>/` across libraries; `config.vdf` CompatToolMapping; `localconfig.vdf` LaunchOptions (newest userdata file wins).
- Heroic: `GamesConfig/<app>.json` — `winePrefix`, `wineVersion`, `targetExe`, `launcherArgs`, `enviromentOptions`, `wrapperOptions`; installed.json `version` / `buildId`.

Engine folders (exe locate when missing, still set `engine` when the shape matches):

- Unreal: `Binaries/Win64|Win32/*Shipping*.exe`; `Engine/Build/Build.version`; `D3D12/D3D12Core.dll` ⇒ default dx12.
- Unity: `*_Data/` next to `{stem}.exe` / `{stem}.x86_64`; `UnityPlayer.dll` / `.so` (API from that binary’s imports).
- RE Engine: `re_chunk_000.pak` (optional `.patch_*`) or `natives/` + `*.pak`.
- Creation: `Fallout4.exe` / `SkyrimSE.exe` / `SkyrimVR.exe` / `Data/Fallout4.esm` / `Data/Skyrim.esm` (not Starfield / CE2).
- BlackSpace: `bin64/` + `cdt.dll` + `cgraph.dll`.
- Mod Organizer 2: `ModOrganizer.ini` `gameName` / `gamePath` / `N\binary=` (skip tools; prefer the game PE, not `ModOrganizer.exe` or SKSE).
- If still no exe: largest real game binary under install_dir (depth 3), including extensionless ELF; skip uninstallers, launchers, PhysX/Social Club, `vc_redist`.

### Outputs (sqlx)

Store snapshot (refresh every scan, not overridable here):

`exe_path`, `prefix_path`, `proton`, `build`, `launch_options`, `env` (JSON object), `wrapper`

Detected / override (override wins; `--force` clears override after confirm):

`detected_*` / `override_*` for `exe_path`, `platform`, `bitness`, `api`, `extra_apis`, `engine`, `prefix_path`, `proton`, `build`, `exe_version`

`fingerprint` TEXT.

`platform`: `native` | `proton` | `wine`. Steam+PE with no prefix yet → `proton`. Heroic `wineVersion.type`. ELF → `native` from `pe`.

32-bit is stored and printed. Play and prewire select the matching launcher and emit the architecture-specific DirectX type.

### Force

`tuxgt scan --force` and `tuxgt doctor <id> --force` re-run detectors and **clear overrides**. If any override is set: print the fields that will be removed, require confirmation (`y`) or `--yes`. Non-TTY without `--yes` is an error. GUI uses the same rule (`docs/dev/app/gui/game.md`).

### CLI

```
tuxgt scan [--force] [--yes]
tuxgt doctor <id> [--force] [--yes]
tuxgt doctor <id> [--set FIELD=VALUE]... [--unset FIELD]...
```

`--set` / `--unset` write the `override_*` columns (fields: `OVERRIDE_FIELDS`)
without running detectors, then print the doctor lines for that id.
`--set FIELD=` (empty value) and `--unset FIELD` clear that override. Both flags
are repeatable and may be combined; `--set` ops apply first, then `--unset` ops,
so `--unset` wins a same-field conflict. Either flag with `--force` is an error
(`--force` already clears every override). Unknown field → error naming it;
unknown id → `Error::UnknownGame`. Every op is validated before the first
write: a bad field or value fails the whole invocation with no overrides
changed.

```
tuxgt doctor manual:standalone:abcdefgh --set api=dx11
tuxgt doctor manual:standalone:abcdefgh --set api=            # clear api override
tuxgt doctor manual:standalone:abcdefgh --unset platform --set bitness=64
```

Unknown id → error. Does not launch the game. Doctor line per field: `key<TAB>value` or `key<TAB>value<TAB>override|detected|store` when a source is needed. Keys: `id`, `platform`, `bitness`, `api`, `extra_apis`, `engine`, `exe`, `prefix`, `proton`, `build`, `exe_version`, `launch_options`, `env`, `wrapper`.

