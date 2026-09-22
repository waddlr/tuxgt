## Wrappers

Types may gain fields; not an external ABI.

Not in this surface:

| Item | Where |
|---|---|
| GUI wrapper switches | `docs/dev/app/gui/game.md` General → Play setup; session `WRAPPERS=`; gamescope/gamemoderun need Apply (`launch.needs`) |
| Env knobs (`MANGOHUD=1`, …) | `env-knob.md` — a wrapper is argv, never env |
| Store-client wrapper injection (Steam launch options / Heroic `wrapperOptions`) | `apply.md`; one trampoline; session does env + argv |
| Wrapper execution for Steam/Heroic rows | trampoline after Apply; protonfixes hook cannot wrap gamescope |

### Plugin

First-party bundle `wrapper` (label `plugin-wrapper-label` = "Wrappers"),
added to `FIRST_PARTY`. Capability table parallel to `ENV_KNOB_PROVIDERS`:

```
WrapperProvider:
  plugin_id() -> &'static str
  wrappers() -> &'static [WrapperDef]

WrapperDef:
  id: &'static str        # [a-z][a-z0-9-]{0,31}; unique across the loaded table
  label: &'static str     # GUI switch label
  help: &'static str      # one line incl. the command it prepends
  program: &'static str   # looked up on PATH at launch
  args: &'static [&'static str]   # prepended args; may be empty
```

Core walks **enabled** plugins only. Disabled `wrapper`: no defs are offered,
`set`/`unset` resolve as unknown; stored rows stay.

### First-party set (v1)

Table order is outer → inner (first = outermost wrapper).

| id | program | args | help |
|---|---|---|---|
| `gamescope` | `gamescope` | | SteamOS session compositor; wraps the whole launch |
| `gamemode` | `gamemoderun` | | Feral GameMode; CPU governor + scheduling |
| `mangohud` | `mangohud` | | MangoHud overlay (argv preload, OpenGL + Vulkan); composes with the Env tab knob |

### Persistence

sqlx (games db), not `plugins.toml`:

```
game_wrappers(game_id TEXT, wrapper TEXT, PRIMARY KEY(game_id, wrapper))
```

Rows survive game-row deletion (prune or re-add), same rule as `env_knobs`; no cascade.

### Launch merge

Owned (manual) path: after `launch_options` tokens and the store `wrapper` column,
selected wrappers are appended **before** `tuxgt-launcher` (the launcher stays innermost,
so `LD_PRELOAD` still reaches the game):

```
gamescope -- gamemoderun -- mangohud -- tuxgt-launcher -- <runner> -- <exe> <args>
```

A wrapper whose `program` basename already appears in the composed argv is not added
again.

Store (Steam/Heroic) path: dispatch stays `owned: false`. Selected wrappers are written
to the session as `WRAPPERS=` (table order). `tuxgt-launcher` (after Apply) execs them
around `%command%` when handle is on. The protonfixes hook ignores `WRAPPERS`.

### CLI

```
tuxgt wrapper list [<game-id>]
tuxgt wrapper set <game-id> <wrapper>
tuxgt wrapper unset <game-id> <wrapper>
```

- `list` without id: `id<TAB>help` per def. With id: `id<TAB>on|off<TAB>help`, applicable
  rows only (enabled plugin), deterministic table order.
- `set`/`unset`: unknown game, unknown wrapper, or disabled plugin → error; setting an
  already-set wrapper upserts.

### GUI

Game → General → **Extra wrappers**: one switch per def, checked from
`game_wrappers` and toggled through `set_wrapper` / `unset_wrapper`. A toggle re-reads the
stored set, rewrites session `WRAPPERS=`. Help is a tooltip, not body copy.
gamescope/`gamemoderun`/mangohud-as-argv need Update Launch Options
(`launch.needs`); the protonfixes hook cannot wrap them. Enabling one while
Hook is armed auto-switches to Update Launch Options. Dispatch stays `owned: false`.
