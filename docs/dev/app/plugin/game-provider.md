## GameProvider

Types may gain fields; not an external ABI.

Not in this surface (recorded so they are not forgotten):

| Item | Where |
|---|---|
| `apply()` body (Steam/Heroic launch-config write + backup + restore) | `apply.md` |
| `tuxgt launch --apply` | `apply.md` |
| Current launch config **write**, playtime | `launch-play.md` / `apply.md` |
| Launch config **read** (options, env, wrapper, Proton pick, prefix) | `detector.md` |
| Detector exe / API / engine | `detector.md` |
| SteamGridDB fallback art | `metadata.md` |
| GUI library cards | `docs/dev/app/gui/library.md` |
| `tuxgt game show` | `metadata.md` |

`apply()` **exists** as a default method that returns `ApplyUnsupported` and is **not called** in this surface. Manual never gains a body. Steam/Heroic override in `apply.md`.

### Plugins

First-party `GameProvider` bundles, attached by plugin id (same id grammar as the host):

| Plugin | Manager |
|---|---|
| `steam` | Steam apps + non-Steam shortcuts (standalone rows) |
| `heroic` | one manager; stores `gog` / `epic` / `amazon` / `standalone` (sideloads) |
| `manual` | TuxGT-only entries |

Disabled plugin: scan (and `games add` for `manual`) must not run that provider. Existing sqlx rows stay.

### Trait

Sync. sqlx stays async in core. No launch-config method.

```
GameId:
  manager: String
  store: String     # empty when manager is the store
  game: String      # non-empty; may contain ':' (split only the first two ':')
  display: manager:store:game     # steam::814380, steam:standalone:<appid>, heroic:gog:<id>, manual:standalone:<8id>

GameRecord:
  id: GameId
  name: String
  install_dir: Option<PathBuf>
  cover_path: Option<PathBuf>     # absolute, file exists, or None
  header_path: Option<PathBuf>

GameProvider:
  plugin_id() -> &'static str
  scan() -> Result<Vec<GameRecord>>
  apply(game_id: &str) -> Result<()>   # default ApplyUnsupported; not called in this surface
```

Static table keyed by plugin id, parallel to `FIRST_PARTY`. Core walks **enabled** plugins only.

Missing Steam/Heroic roots: log and continue with empty records; not a hard error. One bad file: log and continue.

### Scan vs watch

Scan only. `tuxgt scan` is a full pass of enabled providers. No `watch` method. No `notify`. Folder watch is not planned.

### Upsert

Provider yields records; **core** writes sqlx. Primary key is the display triple.

```
games:
  id TEXT PRIMARY KEY          # display triple
  manager TEXT NOT NULL
  store TEXT NOT NULL          # '' when empty
  game_id TEXT NOT NULL
  name TEXT
  install_dir TEXT
  cover_path TEXT
  header_path TEXT
  steam_appid TEXT             # user Steam-AppID overlay; upsert never updates it
```

FTS5 virtual table on `name`, rebuilt after scan and after `games add`.

Prune: for `steam` and `heroic`, if that plugin was enabled and scanned this pass, delete that manager’s rows whose ids were not in the scan set (empty Steam install ⇒ all steam rows go). **Never** delete `manual` rows on scan. Disabled provider: do not scan, do not prune that manager.

8-char ids (`A-Za-z0-9`): only when the store does not already give an id. `base62(sha256(stable key))`, first 8 chars, next 8 on collision against ids already taken. Manual key = canonical exe path; the seed prefix is the display prefix (`manual:standalone:`). If windows run out, append a decimal counter. Steam shortcuts use Steam’s u32 `appid` from `shortcuts.vdf`, not an 8id.

### Artwork

Absolute filesystem paths only for Steam/Heroic local files. Never store bytes/blobs in sqlite. Steam: `appcache/librarycache` (dir layout then flat `{appid}_…`; cover: `library_600x900` then capsule; header: **`library_hero` first**, then `library_header` / `header`) stays outside PREFIX. Game-page wash uses header/hero only, never the portrait cover. Heroic: `file://` or existing local `art_square` / `art_cover` from library JSON, else the remote URL as-is — the GUI fetches it into `$PREFIX/config/cache/art/<game-id-safe>/cover` after first paint. Cap 1024 dirs, LRU by mtime; delete on game remove. Missing art ⇒ NULL.

### Steam / Heroic / Manual

- Steam: `steamlocate::locate_all()` (native + Flatpak). Libraries via `libraryfolders` / `appmanifest_*.acf`. Skip tools and Proton redistributables (name: Proton*, Steam Linux Runtime*, Steamworks Common Redistributables, Steamworks *, Creation Kit; appids including SteamVR and the SLR/Proton tool list). Apps `steam::<appid>`. Shortcuts `steam:standalone:<appid>` using the VDF u32 (same as `compatdata/<appid>/`, grid `{appid}p`, `localconfig` app key). Same appid across native/Flatpak: first wins.
- Heroic: native `~/.config/heroic` and Flatpak `~/.var/app/com.heroicgameslauncher.hgl/config/heroic`; Epic also `legendary/installed.json` next to that config. Installed only. Runner map: `gog`→`gog`, `legendary`→`epic`, `nile`→`amazon`, `sideload`→`standalone`. Skip other runners. Skip GOG `gog-redist`.
- Manual: `scan()` returns empty. `tuxgt games add <exe>` inserts `manual:standalone:<8id>` (name = file stem, install_dir = parent). Re-add of the same canonical path upserts the same id. There is no GUI add. Manual rows offer GUI removal via `remove_manual`.

Crate: `steamlocate` (brings `keyvalues-serde`). Extra VDF parsing uses `keyvalues-parser` (already a steamlocate dep).

### CLI

```
tuxgt scan
tuxgt games list [--manager] [--store] [--query] [--hidden] [--all]
tuxgt games add <exe>
tuxgt games hide <id> [--clear]
tuxgt games unhide <id>
tuxgt games appid <id> [<appid>|--clear]
tuxgt games appid-search <name>
```

`--query` is FTS5 on `name`. List line: `id<TAB>name<TAB>cover_path<TAB>hidden` (`hidden` marker when the effective flag is set; empty otherwise). The list hides hidden titles unless `--all` (`--hidden` shows hidden only). `hide` stores an override (`hidden`); `unhide` forces visible; `hide --clear` drops the override so the store's detected flag wins. `scan` upserts then prints the same list. `--force` / `--yes` on scan: Detector section.

`games appid` prints `id<TAB>appid` (empty when unset), stores a trimmed 1-10 ASCII-digit appid, or clears it with `--clear`. Unknown id → `unknown game`; bad value → `invalid override`. The column is INSERT-only in the scan upsert (never in the update set), so the overlay survives rescan; a pruned row loses it. Metadata resolution prefers the overlay over the Steam game segment (`metadata.md`).

