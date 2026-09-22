## Host

Types may gain fields; this is not a frozen schema and not an external ABI. Git fetch / catalog install is not this surface.

Not in this surface: `GameProvider`, types, instances/recipes, downloader, launch.

### Layers

| Layer | In `tuxgt plugins list`? | When |
|---|---|---|
| In-tree first-party bundle | yes | compiled into the host |
| Recipe TOML (a Mod of an existing Provides kind, no code) | no | `tuxgt mods` |
| Git-repo catalog plugin | yes, later | after-core; same id grammar; not loaded |

### Identity

```
plugin_id := [registry "/"] name [":" tag]
registry  := host "/" owner-or-path     # github.com/waddlr, gitlab.com/group/sub, codeberg.org/user
name      := [a-z][a-z0-9-]{0,31}
tag       := [A-Za-z0-9._/-]+           # git tag, branch, or ref; also a first-party channel
```

- No UUIDs. Reserved name: `core` (any registry).
- First-party: omit `registry`. Omit `tag` when latest stable (the compiled-in build). Listed and stored as `steam`, not `steam:stable`.
- Remote later: `github.com/waddlr/lutris:v1.2.0` — registry embeds the clone endpoint (`https://{registry}/{name}` at `{tag}`). Not GitHub-only.
- Tag on first-party is not a git ref and does not select another binary. Tag is a git ref only when `registry` is set.
- Plugin ids and Mod ids are different namespaces.
- Display omits empty registry/tag. Match on enable/disable is exact on that display string (`steam` does not imply `steam:beta`).

`label_id` is a Fluent catalog key for the display name. It is **not** the plugin slug (`/` and `:` are not Fluent ids). First-party convention: `plugin-<name>-label` using only the `name` segment (`steam` → `plugin-steam-label`). Registry and tag are not part of the key. List prints the resolved string. Remote plugins later ship their own label string; that is not this field.

Production `FIRST_PARTY`: `steam`, `heroic`, `manual`, `env`, `protondb`, `steamgriddb`, `awacy`, `wrapper`.

Categories (Settings → Core Plugins chrome): Game Manager / Store (`steam`, `heroic`, `manual`); Metadata Providers (`protondb`, `steamgriddb`, `awacy`); Capabilities (`env`, `wrapper`); Mods (official ReShade and OptiScaler Mod cards — not PluginHost rows, so never `FIRST_PARTY` ids). ReShade and OptiScaler are Mods, not plugins: do not add PluginHost ids `reshade` / `optiscaler`.

### Registration

Static table in `tuxgt-core`. No builder, no `inventory`/`linkme`, no `cdylib`.

```
PluginOrigin: FirstParty   # later: Git, Local, … — source class, not a capability type

PluginDesc:
  registry: Option<&str>
  name: &str
  tag: Option<&str>          # None = latest stable
  origin: PluginOrigin     # FirstParty = compiled into this binary; not a git registry
  label_id: &str           # Fluent key; first-party: plugin-<name>-label
  author: &str               # "" ok
  description_id: &str       # Fluent; "" = none
  labels: &[&str]            # zero or more; empty slice is fine
  requires: Option<&[&str]>  # see below
```

`FIRST_PARTY: &[PluginDesc]` — steam, heroic, manual, env, protondb, steamgriddb, awacy, wrapper. Production `PluginHost::load()` uses that table + config. Tests use `PluginHost::load_with(descs, config_dir)`.

`PluginOrigin::FirstParty` means the bundle is compiled into this binary. `registry` is the git-host/owner prefix on a catalog slug. First-party omits `registry`; a `FirstParty` descriptor with `registry: Some(_)` is invalid. Tag on first-party is a channel (`steam:beta`), not a git ref.

Duplicate display ids in the loaded table: keep the first, ignore later copies. Do not error. Log `tracing::warn` with the id, table indexes, origin, author, and `label_id` of both rows.

Capabilities (GameProvider, ModType, …) attach by plugin id.

### `requires`

Plugin-level dependencies (other plugin ids, display form). Not the mod-package graph.

- No dependencies: omit from any on-disk plugin metadata; model is `None`.
- Has dependencies: `Some` of one or more plugin ids. `Some(&[])` is invalid — never store a non-null empty list.
- Callers check once: `None` or non-empty `Some`. Do not also test emptiness.
- The field is recorded and not resolved: no enable cascade, no error if a required id is missing from the table.

### Persistence

Source of truth: `<config>/plugins.toml` (`TUXGT_CONFIG`, else `$PREFIX/config`; dedicated file always; not sqlx; not a combined `config.toml`).

```
disabled = ["steam"]
```

Missing file or missing id ⇒ enabled. File is created only on `enable`/`disable`. Ids in `disabled` that are not in the loaded table are kept on disk and not listed.

Config dir: `TUXGT_CONFIG` if set, else `$PREFIX/config`.

### Enable / disable

- Disable: still listed; `enabled = false`. Later surfaces must not offer that plugin’s capabilities.
- Do not delete sqlx rows, manifests, recipes, or downloads. Re-enable restores capabilities; data remains.
- Default: compiled first-party plugins enabled.
- Host API is the product surface. CLI is one caller. GUI Settings → Core Plugins (`docs/dev/app/gui/settings.md`) calls the same `list` / `set_enabled`. No second store. Git-repo catalog install (add registry → list → install) is Later on that same page (`gui.plugin-registry`); not this surface.

```
tuxgt plugins list
tuxgt plugins enable <id>
tuxgt plugins disable <id>
```

Unknown id (not in the loaded table) → error. Idempotent if already in the requested state. List: empty message, or `id<TAB>label<TAB>enabled|disabled`. Extra metadata stays on the type for GUI.

