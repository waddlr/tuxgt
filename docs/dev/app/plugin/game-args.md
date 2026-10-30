## Game arguments

Per-game executable arguments (tokens after `%command%`, such as `-dx11`). A third channel: not env knobs, not argv wrappers. Nothing here auto-enables. Displaying a row never writes Steam or Heroic.

Not in this surface:

| Item | Where |
|---|---|
| Env knobs and custom env | `env-knob.md` |
| gamescope / GameMode / MangoHud argv | `wrapper.md` |
| Trampoline and session env | `launch-play.md`, `docs/dev/launcher/overview.md` |
| Apply of the trampoline itself | `apply.md` |
| ProtonDB report notes | `metadata.md` (`metadata.protondb-notes-ai`, Later) |

Surfaces (Exists):

| Surface | What |
|---|---|
| `game.args` | Catalog load, merge, working copy, suppression |
| `args.catalog` | Shipped data files + `args-check` |
| `launch.args-delivery` | `LaunchNeeds.args`, Save, manual `LaunchSpec` |
| `gui.game-args` | General group, Column 2 Row 3 (under Launch Mode) |
| `args.harvest` | Offline maintainer tool. PCGW and Lutris inputs stay unbuilt |

### Catalog on disk

Repo root `args/`, sibling of `mods/`. `make prepare` copies `args/curated/` and `args/protonfixes/` into `$(PREPARE)/share/args/` (mode 644). `args/protondb/` ships the same way once a reviewed harvest exists. The app reads `data_dir()/share/args/` at startup. A parse error is logged and that file is skipped whole. No panic, no half-file.

```
args/curated/games/<slug>.toml
args/curated/engines/<engine>.toml
args/protonfixes/games/<id>.toml
args/protondb/games/<appid>.toml    # harvest output, ODbL directory
```

No `godot` file. Detect emits `unreal`, `unity`, `re_engine`, `creation`, `blackspace` only. An engine file exists only for an id detect already emits. `args/pcgw/` and `args/lutris/` are not part of v1; both wait on an owner licence decision. ProtonDB output is ODbL (database) / DbCL (contents), pinned dump, attribution in that directory's `LICENSE`. It stays physically separate from `curated/`.

Hand edits only under `args/curated/`. Other directories are tool-owned.

### File schema

Per-game:

```
title = "Example Game"          # display only

[game]
steam_appid = "1234560"         # optional
gog_id = "example_game"         # optional
epic_id = "abc123"              # optional
exe_names = ["ExampleGame.exe"] # optional stems; require engine
engine = "unreal"               # required when exe_names is set

[[arg]]
id = "dx11"                     # [a-z][a-z0-9-]{0,47}, unique in the file
template = "-dx11"
group = "api"                   # optional; members are single-select
help = "Force DirectX 11."
source_url = "https://example.invalid/thread"
```

`kind` is `flag` (default) | `enum` | `freeform`. `flag` forbids `{value}` and `[[arg.value]]`. `enum` requires one or more `[[arg.value]]` tables (`value`, `help`). `freeform` forbids values. A non-flag template contains exactly one `{value}`.

Engine files use `[engine] name = "unreal"` and no `[game]` table. Same `[[arg]]` tables.

`help` is required. `source_url` is required on curated per-game args. Harvest files carry dump or commit and date in a header comment instead. Unknown keys are errors. `id` is the CLI and sqlx key.

Normalized stem: strip leading `-`, ASCII case-fold, keep the text before `=` or `{value}`. A per-game stem may repeat an engine stem; the per-game row wins. Two rows of the same rank with the same stem are a validator error. `group` is `[a-z][a-z0-9-]{0,31}` when set. `freeform` cannot set `group` (no per-row text input in the GUI). Members of a group are different stems and at most one is enabled, including none.

Templates reject whitespace, quotes, `$`, `` ` ``, `;`, `|`, `&`, `<`, `>`, `\`, and an absolute path (`/` or a drive letter), outside the single `{value}` placeholder.

protonfixes files are a second schema, not `[[arg]]`: `stem`, `appended_value`, `fix_file`. They never become settable rows.

`args-check` is `src/tuxgt/tools/args-check`. It shares the schema types with the loader. Exit nonzero on any schema or same-rank stem error, with `file:line`. Contributors run that bin. `args/README.md` lands with the first catalog files (`args.catalog`), not before.

### Match and merge

A store id matches alone (`steam_appid` via `identity.steam-overlay` else the Steam segment; `gog_id` / `epic_id` via the Heroic game id). An exe stem matches only together with `engine`, against the effective exe stem and the effective engine (override, else detected). An exe stem alone matches nothing. Engine files match the effective engine id exactly.

Trust, highest first: curated per-game, curated engine, pcgw, protondb, lutris. `protonfixes/` is not in that list. Same stem keeps the highest-trust row.

Suppression is decided at read time, never at scan. Order, first hit wins:

1. Literal `util.append_argument` strings in the gamefix file inside the Proton install that will run the game.
2. Else the same literals in umu's gamefix file, when this launch goes through umu (Heroic, or a manual row whose runner is `umu-run`).
3. Else the shipped `share/args/protonfixes/` annotation.

A missing file falls through. A file that is present decides the whole set for this game, including when that set is empty; it does not merge with the later sources. Official Proton with no fix file and no umu hides nothing. Only literal string arguments count. A fix that builds the string in code is not guessed. `util.replace_command` is reported by harvest and ignored here. Steam `cmdlineappend` inside the client's `appinfo` cache is an accepted gap; do not read it.

A suppressed stem is display-only ("applied by Proton"), omitted from composition even when a stored row enables it, and refused for custom text with an error that names the appended value. The stored row stays. Switching Proton re-evaluates. `proton_ge_cachy` is the Hook flavor test only. It is not this lookup.

### Persistence

Same style as `env_knobs`: `CREATE TABLE IF NOT EXISTS` on open, rows survive game-row deletion.

```
game_args(game_id TEXT, arg TEXT, value TEXT,
          enabled INTEGER NOT NULL DEFAULT 1,
          PRIMARY KEY(game_id, arg))
game_args_custom(game_id TEXT PRIMARY KEY, args TEXT)
game_args_published(game_id TEXT PRIMARY KEY,
                    tail TEXT NOT NULL,
                    snapshot TEXT NOT NULL)
```

`game_args` and `game_args_custom` are the working copy and move with the active profile, parked in that profile's `knobs.toml`. `game_args_published` stays on the game. `value` is empty for a flag. `game_args_custom.args` is one string, split with `tokenize()` only. `parse_launch_options` is the wrong parser: text left of `%command%` would become wrappers and env. A `%command%` in the custom string is an error. Empty after trim is an error.

`snapshot` is JSON `{ "args": [{ "arg", "value", "enabled" }], "custom": "" }` taken at Save. `tail` is the exact argument string Save wrote, tokens joined by a single space, each passed through `quote_token`. Empty `tail` means the last Save published no arguments.

No global table.

### Delivery

Composable tokens, in order: merged catalog rows that are enabled and not suppressed, then custom tokens. A `group` contributes at most one member. Dedupe against the user's own game tokens. Same stem drops our token. A different value on that stem keeps the user's token; `list` marks the row `kept-user`. Never dedupe against the post-Save store string, and never against Heroic `wrapperOptions`.

The user baseline is:

- Steam: the post-`%command%` tokens of `ApplyFile.previous` when the record exists, otherwise the live `LaunchOptions`.
- Heroic: the snapshotted `launcherArgs` when the record has that key, otherwise the live `launcherArgs`.

**Manual.** `build_launch_spec` appends the tokens after existing `extra_args`. `owned: true` only. `tuxgt launch --print` shows them because it prints `LaunchSpec::display`. There is no Save. `save` and `discard` error.

**Steam and Heroic.** sqlite updates on `set`. The store is written only on Save. Dirty when the manager is `steam` or `heroic` and the composable tail differs from `game_args_published.tail` (a missing row compares as `""`). A never-saved row with a non-empty tail is dirty.

Save:

- Confirms stop-write-restart through the same path as `launch --apply`. No write while that client runs without the confirm.
- Not dirty → report already saved. Manual → error.
- When no apply record exists, Save arms Apply (handle off, then the trampoline) and publishes the tail in that same write.
- When a record exists, rebuild the published game tokens from the user baseline plus the new tail. Do not append onto the live string. Do not replace `previous`.
- Steam rebuilds `LaunchOptions` as today's `compose_options(previous, trampoline)` plus the new tail.
- Heroic leaves the already-snapshotted `wrapperOptions` alone and sets `launcherArgs` from its baseline plus the new tail.
- The "already applied" short-circuit (launcher fragment present) must not return before that rewrite when the tail differs.
- One client restart per Save.
- Then upsert `game_args_published`.

Discard copies `snapshot` back over `game_args` and `game_args_custom`. No snapshot deletes the working rows. Discard does not touch the store.

Steam surgery stays byte-preserving on `localconfig.vdf` and `shortcuts.vdf`. The new tail compose lives in a sibling of `provider/steam/vdf.rs` (`vdf.rs` is at 374 lines). Result shape: today's `compose_options` (trampoline before `%command%`, user tokens kept), then the published tail after the user's post-command tokens.

Heroic Apply today writes `wrapperOptions` only. Save also merges the tail into `launcherArgs`. `provider/heroic/apply.rs` is near the size target; the merge goes in a sibling. The apply snapshot gains a `launcherArgs` key the first time Save touches it, holding the pre-Save value (null when the key was absent). Later Saves do not replace that baseline. Restore writes snapshotted keys only. An old record with no `launcherArgs` key must not remove the user's `launcherArgs`. `gui/reload` syncs `launcherArgs` once Save writes it.

`launch --restore` and Not hooked restore the pre-TuxGT fragment and delete `game_args_published`. The working copy stays. If it still composes a tail, the row is dirty again.

Session file does not gain arguments. The trampoline does not append argv.

Store proof is the compose/restore fixtures plus in-game smoke. Do not append catalog tokens onto `steam://rungameid/…` or `heroic://launch?…`. Tests use fixtures, never a live `localconfig`.

### LaunchNeeds

`LaunchNeeds` gains `args: bool`, true when any enabled, unsuppressed catalog row or non-empty custom token exists. `from_state` takes it. `channel_needed` and `apply_legal` include it. `hook_legal` is false whenever it is true, including when preload is also on (same shape as `argv_wrappers`). `install_only` stays false when it is set. `game_launch_needs` reads the working copy.

Manual rows never call `apply_launch`.

Toggling an arg does not change Launch Mode and does not restart the client. Save is the write. While Hook is armed and `args` is on, Hook paints disabled. Save performs the Hook → Apply move inside the one confirm.

Auto-restore runs only when no need remains. Dropping the last arg while preload or install still needs a channel does not drop the trampoline. Enable & Play, when Hook is illegal because `args` is on, Saves when the row is dirty and otherwise arms Apply, then Plays. A dirty Save is one confirm and one restart.

The needs table in `docs/dev/app/core/launch.md` has the row: args → Hook no, Update Launch Options required, Not hooked legal.

### CLI

Mirror `tuxgt env` (tab-separated, errors on unknown ids and values):

```
tuxgt args list [<id>]
tuxgt args get <id> <arg>
tuxgt args set <id> <arg> [value]
tuxgt args unset <id> <arg>
tuxgt args enable <id> <arg>
tuxgt args disable <id> <arg>
tuxgt args custom list <id>
tuxgt args custom set <id> "<args>"
tuxgt args custom clear <id>
tuxgt args save <id> [--yes]
tuxgt args discard <id>
```

`list` with no id prints catalog rows: `file`, `id`, `kind`, `help`. `list <id>` prints `dirty` or `clean` on the first line for a store row (`clean` for manual), then `id`, `kind`, `value`, `enabled|disabled`, `source`, `mark`, `help`. `mark` is empty, `suppressed`, or `kept-user`. Suppressed rows are not settable.

`set` of a flag takes no value. `enum` requires a listed value. `freeform` requires a non-empty value. `set` or `enable` of a grouped arg enables that member and disables the others in the group. `set` of a suppressed arg errors and names the appended value. `unset` removes the row. `disable` keeps the value and omits the arg. `save --yes` confirms the store-client stop and restart. Without `--yes`, Save asks. A clean row prints `{id} already saved` and does not stop the client.

### GUI

`gui.game-args`, a sibling of `gui/game/env.rs`, painted from General. One **Launch Arguments** group on General, Column 2 Row 3 (under Launch Mode). Reuse `labeled_row`. Ungrouped flag = switch. Enum = value menu. A `group` is one control: none, then each member (an enum member opens that member's values). Help is the catalog string, tooltip on the label, source link when `source_url` is set. Per-arg help stays en-US in the TOML. Chrome goes through `Strings::get` and the `GUI_IDS` allowlist:

| Id | en-US |
|---|---|
| `gui-args-section` | Launch Arguments |
| `gui-args-applied` | Applied by Proton |
| `gui-args-save` | Save |
| `gui-args-discard` | Discard |
| `gui-args-dirty` | The launcher will use these after Save. |
| `gui-args-none` | None |
| `gui-hero-args` | { $n } args |
| `gui-mode-hook-args` | Hook can't pass game arguments. |

Suppressed rows sit in a muted **Applied by Proton** list (argument plus the appended value). No switch. They do not count as modifications.

Freeform values are CLI-set. The row shows the value read-only and the `tuxgt args set` line in the tooltip.

Dirty store rows show Save and Discard on the group. Save is the store write. The hero pill does not write. Clicking `{n} args` opens General. The pill carries a chevron.

Hero (`gui.game-hero`): count enabled unsuppressed args that compose, plus one when custom text is non-empty. Count 0 adds no pill. When mods or knobs are shown, a count above zero adds `{n} args` beside them. When both are zero, that pill replaces Unmodified. Suppressed rows are not in the count.

General loads the argument rows. Other tabs that already load knob counts also load this count. The section is on General, so the titlebar stays `{Manager} / {display name}`.

### Harvest

`src/tuxgt/tools/args-harvest`. Maintainer-run. Never called by the app, never at scan. Output is a diff for review. It does not commit and does not enable anything.

Build order when `args.harvest` starts: protonfixes annotation first, then a ProtonDB schema spike on two appids. If the pinned dump has no structured launch field, stop. Do not mine free-text notes in v1. PCGW and Lutris stay unbuilt until the owner clears the licence.

The tool still emits a settable row when protonfixes appends that stem, and lists it in the summary as hidden while that fix runs. It does not edit `args/curated/`. Every emitted file passes `args-check`. Pins (dump sha256, protonfixes commit, date) live in the generated header.

### Out of v1

No runtime fetch of ProtonDB, PCGamingWiki, or Lutris. No scan-time work. No detector changes. No hook or trampoline argv channel. No global args. No risk field, no API gate, no per-arg confirm dialog. No `appinfo.vdf` launch entries, no exe string mining, no `--help` probing. `replace_command` and Steam `cmdlineappend` stay gaps.
