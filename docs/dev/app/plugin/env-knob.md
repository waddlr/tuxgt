## EnvKnob

Types may gain fields; not an external ABI.

Not in this surface (recorded so they are not forgotten):

| Item | Where |
|---|---|
| GUI Env tab | `docs/dev/app/gui/game.md` |
| GUI Settings Env (global knobs) | `docs/dev/app/gui/settings.md` |
| Host GPU vendors | `detector.md` |
| Knob/custom-env merge into `LaunchSpec` | `launch-play.md` |
| Store `env` JSON snapshot (providers) | `detector.md`; scan refreshes every pass |
| Help strings in the Fluent catalog | later (static en-US strings for now) |

### Plugin

First-party bundle `env` (label `plugin-env-label` = "Env knobs"), added to `FIRST_PARTY`. Capability table parallel to `GAME_PROVIDERS`:

```
EnvKnobProvider:
  plugin_id() -> &'static str
  knobs() -> &'static [EnvKnob]
```

Static table in `tuxgt-core`. Core walks **enabled** plugins only. Disabled `env` plugin: its knobs are not offered — `list` shows none and `set`/`get`/`unset` resolve as unknown; stored knob rows stay. `env custom` is core, not a capability, and still works.

### Knob schema

```
Scope: proton | wine | native
Flavor: ge_cachy                 # proton-ge / proton-cachyos; empty flavors = any

EnvKnob:
  id: &str                 # [a-z][a-z0-9-]{0,31}; unique across the loaded table; CLI/sqlx key
  help: &str               # description incl. a value sample; static en-US
  scopes: &[Scope]         # empty = any platform (e.g. MangoHud)
  flavors: &[Flavor]       # empty = any Proton; GeCachy = proton-ge / proton-cachyos only
  freeform: Option<&str>   # Some(var) => any string value sets VAR=<value>
  values: &[EnvKnobValue]  # allowed values; empty when freeform

EnvKnobValue:
  value: &str
  help: &str
  env: &[(&str, &str)]     # VAR -> value; one value may set several vars
```

- Exactly one of `freeform` or non-empty `values`. `freeform` var name follows env-assign grammar (`[A-Za-z_][A-Za-z0-9_]*`).
- GUI **label is the env var**, not `id` (`DXVK_HUD` not `dxvk-hud`). Freeform → that var; listed values that share one var → that var; multi-var → vars joined with ` · `. Freeform **value** set is CLI (`tuxgt env set` / `tuxgt env global set`); gpui-kit has no per-knob `Input` (`docs/agent/landmines.md`).
- Unset knob ⇒ no stored value, no env written. Defaults stay the runner's/proton's; a knob never silently writes an unset var.
- **enabled / disabled**: a **set** knob may be disabled. The value stays; session file and launch omit its vars (treat as unset). Disable at the **game** layer is not unset: it means no override (inherit global/unmanaged).
- **Game override-off** (single-value boolean switch off, checkbox on): an enabled game row with an empty value. Session and `LaunchSpec` write `VAR=` for that knob's vars so they win over enabled globals / unmanaged. Unset (no row) still inherits. A 0/1 toggle off writes `0`, not override-off.
- Multi-var values exist (`nvidia-prime-offload` sets three vars).
- `scopes` is a set-time/app-UI gate only. Launch does not filter on scopes; an inert var is harmless. GUI/CLI must not offer non-applicable knobs.

### First-party set (v1)

Curated from GOverlay's tweak list plus official Proton 11, proton-cachyos, GE-Proton, DXVK, vkd3d-proton, MangoHud, Mesa and NVIDIA driver docs. Retired aliases are excluded (e.g. `PROTON_ENABLE_HDR` on cachyos → `DXVK_HDR`; knob set follows GE/upstream naming). Debug/fork-only vars are exposed as freeform knobs, not value lists.

| Group | Knobs |
|---|---|
| proton | `proton-log` (freeform), `proton-wined3d`, `proton-no-d3d11`, `proton-no-esync`, `proton-no-fsync`, `proton-no-ntsync`, `proton-no-write-watch` (GE/Cachy), `proton-wow64`, `proton-large-address`, `proton-heap-delay-free`, `proton-old-gl-string`, `proton-nvapi`, `proton-nvapi-off`, `proton-hide-nvidia-gpu`, `proton-ngx-updater`, `proton-wayland` (GE/Cachy), `proton-hdr` (GE/Cachy), `proton-sdl-input`, `proton-fsr4` (freeform, GE/Cachy), `proton-dlss-upgrade` (freeform, GE/Cachy), `proton-xess-upgrade` (GE/Cachy), `proton-local-shader-cache` (GE/Cachy), `proton-nvidia-libs` (GE/Cachy), `proton-nvidia-libs-no-32bit` (GE/Cachy), `proton-nvidia-nvoptix` (GE/Cachy), `steamdeck-spoof` (all proton; spoof scopeless) |
| wine | `wine-dlloverrides` (freeform), `wine-debug` (freeform), `wine-sync` (esync/fsync/ntsync → `WINEESYNC`/`WINEFSYNC`/`WINENTSYNC`), `wine-fsr`, `wine-integer-scaling` |
| dxvk | `dxvk-hud` (1/full/0), `dxvk-config` (freeform), `dxvk-async`, `dxvk-filter-device` (freeform), `dxvk-vkreflex` |
| vkd3d | `vkd3d-config` (freeform), `vkd3d-framerate` (freeform), `vkd3d-present-mode` (IMMEDIATE/MAILBOX/FIFO/FIFO_RELAXED/FIFO_LATEST_READY) |
| mangohud | `mangohud`, `mangohud-config` (freeform) — scopeless |
| mesa | `mesa-present-mode` (fifo/relaxed/mailbox/immediate), `mesa-shader-cache-size` (freeform), `mesa-driver-override` (freeform), `mesa-gl-version` (freeform), `mesa-anti-lag`, `vblank-mode` (0/1/2/3) |
| amd | `radv-perftest` (freeform), `radv-debug` (freeform), `radv-force-vrs` (2x2/1x2/2x1/1x1), `amd-debug` (freeform), `dri-prime` (freeform) |
| intel | `anv-debug` (freeform), `intel-debug` (freeform) |
| nvidia | `nvidia-prime-offload` (offload → 3 vars), `nvidia-vrr` (1/0), `nvidia-vsync-gl` (0/1), `nvidia-threaded-gl` (1/0), `nvidia-shader-cache-skip-cleanup` (`__GL_SHADER_DISK_CACHE_SKIP_CLEANUP`), `nvidia-shader-cache-size` (freeform `__GL_SHADER_DISK_CACHE_SIZE`) |
| general | `malloc-arena-max` (freeform), `low-latency-layer`, `low-latency-layer-reflex` — scopeless |

Scopes: proton knobs = `proton`; wine/dxvk/vkd3d knobs = `proton` + `wine`; mesa/amd/intel/nvidia/general/mangohud = scopeless (host drivers apply under every platform).

GE/Cachy (`Flavor::GeCachy`): game Env puts them in **Advance** when the game proton is not CachyOS/GE (`cachy` / `ge-proton` in the proton string or dir). Global Env keeps them in main. Host GPU (`detect.gpu`): nvidia/amd/intel **groups** whose vendor is not on the host go to Advance; unknown hosts keep all in main. mesa stays in main and mirrors into Advance only on NVIDIA hosts. Game Env with effective api `dx12` sends dxvk to Advance; any other known api sends vkd3d there. Set or unmanaged knobs stay in main.

CPU note: there is no first-party CPU governor env (kernel/daemon side). `MALLOC_ARENA_MAX` is the one real CPU-side env knob; GameMode stays a wrapper (`gamemoderun`; `wrapper.md`), not a knob (it fights `LD_PRELOAD` from the launcher).

### Persistence

sqlx (games db), not `plugins.toml`. User-set, survives scan upserts (unlike the store `env` snapshot).

```
env_knobs(game_id TEXT, knob TEXT, value TEXT, enabled INTEGER NOT NULL DEFAULT 1, PRIMARY KEY(game_id, knob))
env_knobs_global(knob TEXT PRIMARY KEY, value TEXT, enabled INTEGER NOT NULL DEFAULT 1)
env_custom(game_id TEXT, key TEXT, value TEXT, PRIMARY KEY(game_id, key))
```

`enabled` 1/0. Existing `env_knobs` rows migrate to enabled=1. Global knobs are not a `game_id` sentinel.

`env_custom` is **core**, not a knob and not the store snapshot: freeform per-game env pairs, key `[A-Za-z_][A-Za-z0-9_]*`, value any string. This satisfies the docs/dev/app/gui/game.md Env tab split: registered knobs from plugins + freeform extra env from core.

Rows in both tables survive game-row deletion (prune or re-add): store ids are stable and a manual re-add of the same canonical path upserts the same 8id, so the rows stay valid; no cascade delete.

### Platform gate

Effective platform, same pick as launch: `override_platform` else `detected_platform`, else `proton` when prefix or proton is set, else `native`. `set` and `get` refuse a knob whose `scopes` exclude that platform (`knob not applicable: <knob> on <platform>`). `unset` is allowed regardless of scope (cleanup after an override change). Custom env has no gate.

### CLI

```
tuxgt env list [<id>]
tuxgt env get <id> <knob>
tuxgt env set <id> <knob> [value]
tuxgt env unset <id> <knob>
tuxgt env enable <id> <knob>
tuxgt env disable <id> <knob>
tuxgt env global list
tuxgt env global set <knob> [value]
tuxgt env global unset <knob>
tuxgt env global enable <knob>
tuxgt env global disable <knob>
tuxgt env custom list <id>
tuxgt env custom add <id> KEY=VALUE
tuxgt env custom remove <id> KEY
```

- `list` without id: `id<TAB>scopes<TAB>help` (scopes `all` when empty). With id: applicable knobs only, `id<TAB>scopes<TAB>value<TAB>enabled|disabled<TAB>source<TAB>help`; value empty when unset; `source` is `game` / `global` / `unmanaged` / empty.
- `set` without value: knob must have exactly one allowed value (boolean-style), else error. Value must be listed (exact match) or, for freeform knobs, any non-empty string. Unknown knob/game/value → error; setting an already-set knob upserts and leaves enabled=1.
- `get`: `knob<TAB>value<TAB>enabled|disabled`; unset → error.
- `disable` on a set knob keeps the value; unset still errors if not set. `enable` on unset is ok (still writes nothing until set).
- `custom add` parses `KEY=VALUE` (first `=` splits; `KEY=` keeps an empty value). `custom` is not gated on the `env` plugin.

### Global

Sqlx `env_knobs_global` is the only store. No `environment.d`, no `/etc`, no sudo.

Enabled+set globals merge into owned `LaunchSpec.env` and into `games/<rel>/tux-protonfixes.conf` (before per-game knobs; later wins). They apply only when that channel runs: manual Play, Hook (`inject=1`), or Apply trampoline (`inject=0` + launcher in store options). Not hooked Steam/Heroic Play stays vanilla.

Enabled globals are **not** a launch need. They do not show Launch Mode / Enable & Play by themselves. Auto-restore when per-game needs drop to none still disarms even if globals remain.

**Unmanaged:** process env matches a registered knob and there is **no** enabled global. Show the live value; treat as disabled; **not writable** (GUI muted, CLI/core `set`/`enable`/`disable`/`unset` error `knob unmanaged: <id>`). A stored disabled global row behind unmanaged stays in sqlx but cannot be mutated until the live vars are gone. Live mismatch after we already enabled+set is not unmanaged (ours). Per-game Env may still override unmanaged for that game (enable copies the live value into a game row).

On `open_db`/`migrate_env`: if `$XDG_CONFIG_HOME/environment.d/90-tuxgt.conf` (else `~/.config/environment.d/90-tuxgt.conf`) exists and **starts with** `# Written by TuxGT. Do not edit; TuxGT rewrites this file.\n`, delete that file. Foreign files in that dir stay. Do not remove the directory.

### Merge into LaunchSpec

Store `env` JSON first, then **enabled** global knobs, then **enabled** knobs set on that game, then `env_custom` pairs. Later wins. Skip disabled (retain stored value). Launch never re-validates values or scopes — the set-time gate owns applicability, and a stale var is inert. Knob defs resolve from the static table (`find_knob`); skip knobs whose provider is disabled. Unmanaged vars are already in the process; omit-at-launch is enough to inherit them.

The same enabled global knobs, then enabled per-game pairs (plus custom env), then mod env are written into `games/<rel>/tux-protonfixes.conf` by `sync_session` when handle is on, for the protonfixes hook and the trampoline (same later-wins as LaunchSpec). Unmanaged is not written (already in the process).

