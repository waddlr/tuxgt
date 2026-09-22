## MetadataSource

Types may gain fields; not an external ABI.

Not in this surface:

| Item | Where |
|---|---|
| GUI badges / open-site buttons | Library cards; Game hero chips + General → About (`docs/dev/app/gui/game.md`) |
| GUI fetch | `gui.metadata-fetch` — GUI calls `game_show` on select (not on paint); SteamGridDB `hero_url` is the missing-hero wash fallback |
| Art bytes on disk (download) | Background `render_game_art` → `config/cache/art/` (`docs/dev/app/plugin/game-provider.md`) |
| Steam-AppID overlay storage / CLI | `docs/dev/app/core/identity.md`, `docs/dev/app/plugin/game-provider.md` (`tuxgt games appid`) |
| AI on ProtonDB notes | later (`.agents/docs/TASKS.md` roadmap) |
| ProtonDB notes in-app | never in v1 (docs/dev/app/core/overview.md) |

### Plugins

First-party bundles, one per source, attached by plugin id:

| Plugin | Source |
|---|---|
| `protondb` | ProtonDB compatibility tier |
| `steamgriddb` | SteamGridDB fallback art (off until keyring key) |
| `awacy` | AreWeAntiCheatYet warning |

Disabled source: no fetch, no cache write, line omitted from `game show`.

### Trait

Sync, mirroring `GameProvider`. Blocking HTTP lives in `fetch`; the core pipeline calls it via `spawn_blocking` (sqlx stays async). HTTP: `reqwest` (blocking, rustls, verify on), 15s timeout, `tuxgt/<version>` user agent.

```
MetaInput:
  id: GameId
  name: String                  # games row name
  steam_appid: Option<String>   # games.steam_appid overlay, else manager == "steam" → game segment

MetaLine:
  key: &'static str             # plugin id
  value: String
  extra: Option<String>

MetadataSource:
  plugin_id() -> &'static str
  fresh_secs() -> u64                          # cache lifetime
  cache_key(&MetaInput) -> String              # default: display triple; awacy: "" (global)
  fetch(&MetaInput) -> Result<Value>           # blocking HTTP; JSON payload; Value::Null stores nothing
  show(&MetaInput, Option<&Value>) -> Result<Option<MetaLine>>

METADATA_SOURCES: &[&dyn MetadataSource]   # protondb, steamgriddb, awacy
```

Core walks enabled sources only, analogous to `GAME_PROVIDERS`. Parsing lives in pure helpers (fixture-tested); `steamgriddb` reads the keyring in `show`. Fetch errors are `tracing::warn`-ed by the pipeline, stale cache is kept, and a line with no cache prints `none`; keyring errors abort with no lines.

### Sources

- **protondb** (24h): `GET https://www.protondb.com/api/v1/reports/summaries/<appid>.json` → whitelisted payload `{"tier", "trendingTier", "provisionalTier", "bestReportedTier", "confidence", "score", "total"}` (`{"tier": null}`, no other keys, when the report is 404). Effective tier for chips / `show` value: `tier` unless it is missing / null / empty / `pending`, else `provisionalTier`, else `none`. Old `{tier}`-only cache rows stay valid until refresh. Resolves `steam::<appid>`, `steam:standalone:<appid>`, and any row with a stored Steam-AppID overlay (`tuxgt games appid`); rows with neither print no line. Site: `https://www.protondb.com/app/<appid>`. No notes.
- **steamgriddb** (24h): `Authorization: Bearer <key>` (API v2). Steam AppID first (`/games/steam/<appid>`), else name autocomplete (`/search/autocomplete/<name>`, first result), then `/grids/game/<id>` — prefer `600x900`, else first grid — and `/heroes/game/<id>` — prefer `1920x620`, else first hero. Payload `{"art_url": url|null, "hero_url": url|null}`; **URL only, never bytes**. Old `{art_url}`-only cache rows refetch when a key is set. Without a keyring key: no fetch, line `skipped (no key)`. GUI wash uses `hero_url` when there is no local wide `header_path` (`gui.metadata-fetch`).
- **awacy** (7d): whole dataset `https://raw.githubusercontent.com/AreWeAntiCheatYet/AreWeAntiCheatYet/master/games.json`, trimmed to name / steam id / status / anticheats, cached once per machine (cache row `game_id = ''`). Match per game: `storeIds.steam` first, else case-insensitive name. Value: status (`Supported | Running | Broken | Denied | Planned`), no match → `none`; extra: comma-joined anticheat names.

### Key

`keyring` crate → system keyring (docs/dev/app/core/install.md; never plain-text config). Service `tuxgt`, account = source id. Only `steamgriddb` takes a key in v1; other sources error as unknown key sources. `set` reads one line from stdin (echo not suppressed in v1; pipe it), never argv. `clear` removes the entry; clearing a missing entry is ok.

```
tuxgt metadata key set steamgriddb
tuxgt metadata key clear steamgriddb
```

GUI, Settings → Core Plugins → Metadata Providers → SteamGridDB card: the key editor lives on the plugin card (no General API-credentials card). The key lives in the keyring; the pill is `set` /
`not set`. Never render the secret back; no fake `••••`. **unset:** masked input + Save +
Test. **set:** pill + Test + Clear + Edit (no input). **editing:** masked empty input +
Save + Cancel. Save is `secret_manager_set` (empty → error, no write); Clear is
`secret_manager_clear`; both refresh pills and leave editing. Test is
`secret_manager_test(source) -> Result<String>`: missing key → error; otherwise
`GET https://www.steamgriddb.com/api/v2/search/autocomplete/portal` with
`Authorization: Bearer <key>` from the keyring (not the field); 2xx → one-line valid
report, 401/403 → one-line rejected, other status / network failure → error. The key
never appears in a report, error, or log.

### Cache

sqlx, same db:

```
metadata_cache:
  game_id TEXT NOT NULL        # display triple; '' = global (AWACY dataset)
  source TEXT NOT NULL
  data TEXT NOT NULL           # fetch payload JSON
  fetched_at INTEGER NOT NULL  # unix seconds
  PRIMARY KEY (game_id, source)
```

Fetch when the row is missing or older than `fresh_secs()`; `--refresh` forces. Fetch returning `Value::Null` stores nothing.

### CLI

```
tuxgt game show <id> [--refresh]
tuxgt games appid <id> [<appid>|--clear]   # Steam-AppID overlay (identity.md)
tuxgt games appid-search <name>            # keyless Steam Store search, appid<TAB>name lines
```

Unknown id → error. Doctor-style lines, one per enabled source:

```
protondb<TAB><tier|none><TAB>https://www.protondb.com/app/<appid>   # steam rows only
steamgriddb<TAB><art-url|skipped (no key)|none>[<TAB>hero-url]
awacy<TAB><status|none>[<TAB><anticheat,…>]
```

Does not run detectors and does not write the games table.
