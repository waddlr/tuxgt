# Plugin host — engineering contract

Product rules: `docs/dev/app/core/`. If this folder and those files disagree, **core wins** — stop and fix this folder.

Open **one** surface file. Do not ingest this folder. No external ABI.

| File | Surface |
|---|---|
| `host.md` | Plugin id, registry, enable/disable |
| `game-provider.md` | `GameProvider` scan |
| `detector.md` | Detector pipeline (core utility, not a plugin row) |
| `mod-type.md` | `ModType` + graph |
| `instances.md` | Mods + Mod file TOML |
| `download.md` | Downloader + FileManifest |
| `metadata.md` | `MetadataSource` |
| `env-knob.md` | `EnvKnob` |
| `launch-play.md` | `LaunchSpec` + Play + handle/session |
| `launch-adapter.md` | LaunchAdapter + prewire |
| `apply.md` | Apply + harvest |
| `wrapper.md` | Overlay wrappers |

## Locked from core (not renegotiated here)

- Plugin = bundle of capabilities, not 1:1.
- Core is not a plugin. C launcher stays C.
- Layers: recipe TOML → in-tree first-party → external ABI later (do not freeze).
- Mod: `id`, `type` (Provides), `label`, `source`, `plans_allowed`, `requires` (Mod ids), `include`, `slot`. `proton_env` official-only. Official Mods disable-able, not removable; packaged TOML, never literals. User Mods: from-package scan + classify + keep/dest tweak; local payload copied to `$PREFIX/mods/user/<id>/` on Save. Instance = per-game FileManifest + staging.
- Graph in core: `requires`, `conflicts`; no silent dep; no silent DLL pick.
- Env is a plan on the injector, not a third launch adapter. Global knobs: sqlx + session/LaunchSpec merge (enable/disable).
