## ModType + graph

Types may gain fields; not an external ABI.

Not in this surface:

| Item | Where |
|---|---|
| Mods + Mod file TOML | `instances.md` |
| Downloader + FileManifest | `download.md` |
| `LaunchAdapter` preload vs install, prewire | `launch-adapter.md` |
| GUI Mods | `docs/dev/app/gui/game.md` |
| GUI Mods graph / files strip | `docs/dev/app/gui/game.md` |

### Types

Flat (`Provides`): `reshade`, `optiscaler`, `reshade_addon`, `custom`, `effect`, `texture`. Reserved `env_tool` — and any other unknown name, including `injector` and `custom_dll` — is rejected as unknown by `parse_mod_type`.
Provides id `custom` displays as **Custom Mod** (`gui-modtype-custom`); never `custom_dll` / `custom_mod`. `d3dcompiler-47` is `type = "custom"`.

### Trait

`Send + Sync`, mirroring `GameProvider`. One struct per type; static registry of trait objects.

```
ModType:
  mod_type() -> &'static str
  default_slot() -> Option<ProxySlot>       # None = claims no proxy slot
  requires() -> Option<&'static [&'static str]>   # None = none; Some is never empty; type names
  dest_root() -> Option<&'static str>       # default None = dests keep archive-relative path

MOD_TYPES: &[&dyn ModType]   # reshade, optiscaler, reshade_addon, custom, effect, texture
parse_mod_type(name) -> Result<&dyn ModType>   # unknown (incl. reserved `env_tool`) → error
```

| type | `default_slot` | `requires` |
|---|---|---|
| `reshade` | None (stock-named; claims no proxy) | None |
| `optiscaler` | `Dxgi` | None |
| `reshade_addon` | None (`.addon64` discovered) | `reshade` |
| `custom` | None (slot picked per package; never silent) | None |
| `effect` | None (shader files, no load) | `reshade` |
| `texture` | None (texture files, no load) | `reshade` |

`dest_root`: `effect` → `reshade-shaders/Shaders`, `texture` → `reshade-shaders/Textures`, the other four → `None`. Dest rules live in `download.md`. OptiScaler: basename `OptiScaler.dll` → `dxgi.dll` (`default_slot`); not a silent pick. Mixed addon+shader packs take type `reshade_addon`: `.addon64`/`.addon` files dest to the basename root, leftover `.fx`/texture files stay archive-relative `IncludeFile` trees (same as extras-mint addon leftovers); `dest_for` routes by file kind regardless of recipe type, so `effect`/`texture` payloads never land under the addon root.

### Slots

Preload proxy slots are a closed set. Stock-named loads (`ReShade64.dll`) and ASI-install loads (`.asi` via Ultimate ASI Loader) claim **no** slot (`None`) and never conflict.

```
ProxySlot: Dxgi | D3d9 | D3d10 | D3d11 | D3d12 | Winmm | Version
parse_slot(s) -> Result<ProxySlot>   # lowercase stem; `.dll` suffix optional; unknown → error
```

Conflict rule: two packages conflict iff both claim `Some` of the same slot on the preload plan. ASI loads are exempt (they claim no slot).

### Graph (model only)

```
ModPackage:
  name: &str
  type: &str                 # type name; must parse
  slot: Option<ProxySlot>    # explicit per package; never defaulted silently
  requires: &[&str]          # type names

diagnose(packages) -> { missing_requires, slot_conflicts }
```

No resolution. Missing requires → report only (a declined default stays missing). Slot conflict → report both claimants. No silent dep enable. No silent DLL pick.

### CLI

```
tuxgt mods graph
```

Prints a fixture set (reshade; optiscaler on `dxgi`; `tuxgt-nr` addon requiring `reshade`; custom DLL on `dxgi`), one `name<TAB>type<TAB>slot` line per package, then `requires <ok|missing>` / `conflicts <ok|...>` diagnostics. Deterministic order.

