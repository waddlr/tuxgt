## Downloader + FileManifest

Fields may gain; not an external ABI.

Not in this surface:

| Item | Where |
|---|---|
| Play / Apply / harvest body | `launch-play.md`, `apply.md` |
| GUI FileManifest enable | `docs/dev/app/gui/game.md` |
| GUI files strip | `docs/dev/app/gui/game.md` |
| Nexus API | never first-party (browser + local file) |

This is the **app FileManifest = the Instance record**: install-time intent per (game, Mod). Not the loader's runtime status file (`docs/dev/launcher/overview.md`; the C side owns that). Harvest reads both.

### Cache layout (data dir)

Downloads are in-flight only. Keyed by URL hash while the transfer runs; the dir is deleted after the payload lands under `mods/<kind>/<id>/`.

```
<data>/downloads/<sha256(url)[..16]>/
  <filename>            # last URL segment, sanitized; keeps its extension for unpack
  <filename>.part       # partial download; Range resume when server allows
  meta.toml             # url, filename, sha256, bytes, fetched_at
<data>/downloads/downloads.log   # append-only: ts, instance, url, sha256, bytes, result
```

Steady state: `downloads/` empty (log may remain). Cover JPEGs must not live here — they land at `config/cache/art/<game-id-safe>/cover`. Extract always `/tmp/tuxgt-<hash>/`, then copy into the payload dir, then delete tmp.

`meta.toml` records the observed sha256. A recipe may pin `sha256` (enforced when present — mismatch errors, no install). Without a pin: trust on first download, record the hash; any later mismatch (corrupt cache or changed upstream) re-fetches automatically and re-records — the log shows it. `--redownload` forces a re-fetch, replaces the payload dir, then cleans downloads. Subsequent install of the same id skips acquire when the payload dir has files and `--redownload` is off.

FileManifest `source` is a path under `mods/<kind>/<id>/` (archive-relative), not `cache/<hash>#…`. Staging still copies payload → `games/<rel>/stage/`.

### Download

`reqwest` with rustls, TLS verify always on (no skip). `Range` resume from `.part` size when the server answers 206; else restart. When the response has `Content-Length` (206: remaining bytes on top of the `.part`), a short body is a download error — the `.part` is kept for resume, not hashed against a recipe pin. Concurrent `fetch_url` of the same URL waits on one in-flight transfer (one `.part` writer). Hash with `sha2` before install; never install unverified bytes.

Byte progress: `fetch_url` / `acquire_with_source` / `install_instance` take an optional `Option<&(dyn Fn(FetchProgress) + Send + Sync)>` sink (`ProgressSink`). `FetchProgress { bytes, total }` carries the bytes on disk for this transfer (`Content-Length`, plus the `.part` resume offset on a 206) and `total: None` when the server sent no length; `.percent()` is `None` then. One report is emitted before the first chunk (so a bar paints with its total and any resume offset), then one per chunk. The sink stays off `InstallOpts` — that type is per-install intent, while the sink is per-call and not `Clone`. A cache hit / local copy reports nothing (no byte stream to report). CLI passes `None` everywhere; the GUI ferries the reports into a Live toast (`docs/dev/app/gui/game.md` `gui.mods-install`). No cancel, no per-file progress.

GitHub: literal asset name → `/releases/latest/download/<asset>`; glob (`*`, `?`, `[`) → GitHub latest-release API, ASCII case-insensitive match (`github_asset_url`). With `source.tag` both use that release instead: `/releases/download/<tag>/<asset>` and `/releases/tags/<tag>`. Tag is non-empty `[A-Za-z0-9._-]+`, validated at parse. `local` copies the file into the cache (keyed `local:<path>`, same meta/log shape). `manual_url` downloads the pinned URL the same way. `provided` never calls `acquire_with_source`. Install lands from the payload only when every `files` pattern matches exactly one file; otherwise it errors and writes nothing. `--redownload` does not delete that payload and does not fetch. `cache refresh` skips these ids. Provenance `source` is the literal `provided`; `asset_sha256` is the payload tree digest.

Refresh:

```
tuxgt instance install <game-id> <mod-id> [--redownload]
tuxgt cache refresh [<instance-id>]     # re-fetch, no manifest touch
```

`--redownload` forces re-fetch of that asset. `cache refresh` with no id re-fetches all cached assets. Both re-record meta + log.

### Provenance + update check

Every install records `[provenance]` on the manifest: the resolved source
(`source`: download URL, or `local:<abs path>` for disk), the acquired asset
hash (`asset_sha256`), bytes, and fetch time. Reinstalls (same command, with
or without `--redownload`) refresh provenance and preserve per-dest keep bits
for surviving dests.

The core `check_update` compares installed provenance against the current
payload (local sources re-hashed) and hits the network for `Available`
(GitHub asset URL moved on). Results: `UpToDate`, `Available` with detail, or
`Unknown` (manifests with no provenance, or an unresolvable
upstream). A payload on disk with recorded provenance is UpToDate even when
`downloads/` is empty. The manual equivalent of applying an update is:

```
tuxgt instance install <game-id> <mod-id> --redownload
```

### Catalog check

`check_catalog_update(mod_id)` is the catalog-level counterpart: at most one
network hit per Mod (literal github assets and `manual_url` resolve with no
API call; glob/prerelease sources take one release-API resolve; `local`
re-hashes offline). A `provided` source is offline too: an incomplete payload
is `NeedsFiles` (stored status `needs-files`, detail names the missing and
ambiguous patterns); a complete payload with no installed-hash drift is
`UpToDate`; a complete payload whose digest differs from an installed
manifest is `Available` with detail `files replaced`. It records `last_check` + status on the cache row.
Per-game Available stays a local compare
(`manifest.provenance.asset_sha256` vs the cache row); `check_update` stays
the CLI per-game path and, for `provided`, compares the payload digest to the
manifest (`UpToDate`, or `Available` / `files replaced`) with no URL resolve.
Empty payload (never downloaded) is `Unknown`, never
`Available`. A short `provided` payload is `NeedsFiles`, not `Unknown`.

GUI: each installed Mods card shows the check result (`docs/dev/app/gui/game.md`
`gui.mod-update`) — quiet when up to date, Update button when available,
one short unknown note otherwise. The GUI repair path self-heals on
tab load — provenance re-recorded from payload/cache/stage bytes, else a
background redownload reinstall, else one cache fill — and never quotes the
Unknown reason below. The `tuxgt … --redownload` sentence stays as the
**CLI** equivalent only.

### Staged status + force re-sync

`stage_status` reports per-file sync state for every enabled manifest dest of
a game: `in-sync`, `user-modified` (staged copy edited by hand), or
`depot-newer` (the depot moved on — e.g. reinstall while staging stayed — or
the staged copy is missing). Omitted dests are not staged and never
listed. CLI surface:

```
tuxgt instance status <game-id>   # mod<TAB>file<TAB>state per staged dest
```

`resync_instance` / `resync_game` re-copy depot sources into staging.
Without force, user-touched files are left alone and reported via
`Error::StagedModified`; with force they are re-copied. Both return the
post-sync per-file status.

GUI: the Mods tab shows the per-file chips on each installed card and offers
**Force re-sync** per card plus **Force re-sync all** for the game
(`docs/dev/app/gui/game.md` `gui.mod-stage`), with result feedback in the status
line.
Global payload edits push via `push_global_edits`: manifest shas refresh from current payload bytes, then a non-force sync updates in-sync staged files while pre-existing user-modified files are preserved and reported, never fatal. Install tolerates pre-existing touches the same way. A per-game Update redownload replaces depot bytes without a config-edit confirm and keeps per-game staged touches (`force` stays false). Foreign game-dir overwrites still confirm. Pushes and staged edits stop at staging: install-adapter game-dir copies are not re-applied, so those games need a reinstall to load edited configs.

### Unpack via external tools

All archive handling shells out — no unpack crates. Tools are looked up on `PATH`; unpack refuses with a naming error when the tool is missing instead of attempting and failing:

| archive | tool | probe |
|---|---|---|
| `.zip` | `unzip` | `unzip -v` |
| `.rar` | `unrar` | `unrar` (no args prints version) |
| `.7z` | `7z` | `7z` |
| `.cab` (Windows SDK standalone installers) | `7z` | `7z` |
| `.tar.gz` / `.tgz` | `tar` | `tar --version` |
| `.tar.bz2` | `tar` | `tar --version` |
| `.exe` (self-extracting; ReShade Setup) | `7z` | `7z`; if missing or the file is not an archive, copy the exe as a plain dest |

Single files (`.dll`, `.addon64`, …) install directly, no tool needed. `.exe` is SelfExtract first (official ReShade is `ReShade_Setup_*_Addon.exe` from reshade.me, full add-on support; crosire publishes no GitHub assets).

Dest roots: `effect`/`texture` dests are moved under the type's `dest_root` after payload filtering. A leading `reshade-shaders/` is stripped once, then a leading `Shaders/` or `Textures/` maps to its shared ReShade dir (so a pack shipping both keeps them apart); otherwise `dest_root` is prefixed. Recipe `shader_dir` / `texture_dir` (optional) insert one extra path component after that shared dir (`Shaders/foo.fx` + `shader_dir = "OtisFX"` → `reshade-shaders/Shaders/OtisFX/foo.fx`; qUINT is `shader_dir = "qUINT"` and no `texture_dir`). Omitted keys keep the type dests. Payload `keep`/`drop` still match the raw archive-relative path. No `dest_root` → dests unchanged. Explicit `[dests]` still win.

OptiScaler dest: after payload filter, the kept file whose basename is `OptiScaler.dll` (ASCII case-insensitive) dests to `dxgi.dll`. Zero or two matches → install error. Other dests unchanged. Recipe `[dests]` applied after payload filter and **before** these type dest rules; an explicit dest wins.

```
tuxgt cache tools     # name<TAB>found<TAB>version; availability gate for unpack
```

Parent dir: unpack to a temp dir; if the tree has exactly one top-level directory, strip it (contents move up); otherwise keep as-is. GUI: Settings → General **Host tools** shows this table. Plugins registering extra tools is later; the archive table stays hardcoded.

### FileManifest
One TOML per Instance — one (game, Mod) — under `<data>/manifests/<game>_<instance>.toml` (`:` and `/` → `_`). Fields:

```toml
game = "steam::814380"
instance = "optiscaler"
type = "optiscaler"
adapter = "preload"          # preload | install
enabled = true
load_order = 1   # per-game order, this game only; lower stages/loads first, later wins same-dest; missing = 0

[provenance]                  # recorded by every install
source = "https://github.com/..."
asset_sha256 = "…"
asset_bytes = 12345
fetched_at = 1757328000

[[files]]                    # planned files
source = "cache/9f2c…/OptiScaler.zip#OptiScaler.dll"   # cache ref + in-archive path
dest = "dxgi.dll"
sha256 = "…"
enabled = true               # optional; missing = true. Per-game keep; required dests cannot be false

[backups]                    # game-dir files overwritten (restored on uninstall)
"dxgi.dll" = "backups/dxgi.dll.<ts>"

generated_globs = ["ReShade.ini", "ReShade.log", "OptiScaler.ini"]   # harvested after Play
```

Install assigns `max + 1` (later installs win); reinstall keeps its value. Order is per-game install state, never catalog/recipe state.
Install writes the manifest (enabled). `instance enable <game> <mod>` / `instance disable …` flip the **instance** flag. No silent dep enable: installing an addon whose `requires` type has no sibling manifest for that game errors unless `--with-requires <instance>` names the exact instance (installed first, into its own manifest file; still no auto-pick).

### Per-dest keep

`[[files]].enabled` is per dest, **this game only**. Missing key = true. Toggle writes the manifest, stages or drops that dest, and runs `prewire_game`. Omitted dests:

- Do not appear in LoadDLL / IncludeFile
- Are not staged / not copied by the install adapter
- Stay in the depot; re-enable restages
- Uninstall / disable still ignores dests that were never copied (foreign rule)

**Required dests** cannot be omitted (`enabled = false` is an error; GUI switch locked on):

| type | Required dest |
|---|---|
| `reshade` | basename `ReShade64.dll` / `ReShade32.dll` (case-insensitive), or a top-level slot-named dest after a proxy rename |
| `optiscaler` | any slot-named dest (`dxgi.dll` default; `winmm.dll` after `instance slot`) |
| `reshade_addon` | dests ending `.addon` / `.addon64` |
| `custom` | the slot DLL dest |
| `effect` / `texture` | none — every dest is optional; zero kept dests is allowed |

`include` entries ending in `/` cover the whole dest dir: `shaders/` forces IncludeFile for `shaders/foo.fx` and deeper paths. `is_required_dest` and `ini_lines_for` honor the cover: a covered dest is omit-able, except the lone dest of a one-file pack, which stays required.

### Per-dest load mode

`[[files]].load` is per dest, **this game only**. Missing = follow the recipe (`include` → IncludeFile, any other `*.dll` → LoadDLL). `true` forces LoadDLL, `false` forces IncludeFile. Only an applicable dest can be switched: a `*.dll` that is not a `pfx:` prefix copy. The switch does not change `enabled`, required-ness, or which dest claims the slot (those still follow recipe `include`). Reinstall keeps the bit for a surviving source. The same ini-list budget gate as keep applies. `tuxgt instance files` prints the effective `loaddll|include`. Writers: the installed Mods card Load switch, and `tuxgt instance files <game> <mod> loaddll|include <dest>`.

Turning off the last optional dest is fine. Dest match on CLI is the exact `dest` string as stored.

Re-install: dests still present keep their enabled bit; new dests default on; dests gone from the payload drop out. Every re-install refreshes `[provenance]`.

Reinstall matches the prior dest **by source**: a slot rename survives reinstall (a `winmm.dll` dest stays `winmm.dll`), a new source takes the computed dest. A surviving source keeps its keep bit and its `load` bit; a dest the type requires is never omitted.

The managed-ini list budget (8192 bytes per `LoadDLL` / `IncludeFile` list, mirroring the loader) is prevalidated against the prospective manifest before staging or writing it; overflow errors name the budget — omit dests or split the pack, or uninstall the over-budget instance (`tuxgt instance uninstall <game> <mod>`) to recover. A rejected pack leaves no enabled manifest or staging behind, so later installs for the same game still succeed. Custom Mod packs cannot shrink by omitting (every `.dll` is required): uninstall is their only recovery. File toggles ratchet: a toggle that strictly shrinks an over-budget list always lands (the ini rewrites once the lists fit); one that cannot shrink is refused before any write. Nested non-DLL toggles never touch the ini (always-tree). Instance disables use the same gate: only fitting or shrinking disables land, refused before any write.

GUI: keep switches on the installed Mods card (`docs/dev/app/gui/game.md`). Not a raw ini editor. Not a Launch or Settings panel.

### Manifest env

`[[env]]` entries `{key, value, enabled}` (missing `enabled` = true), snapshotted from the recipe `[env]` at install. Reinstall preserves keep bits for surviving keys, defaults new keys on, drops vanished keys. Every env row is optional (no required-env concept). Toggling rewrites the manifest only (no staging, no prewire); launch/session pick the value up via the mod-env merge.

### Harvest

Body is `apply.md` (`harvest_game` / `harvest_all`). This surface only stores `generated_globs` on the manifest at install.

### CLI

```
tuxgt instance install <game-id> <mod-id> [--redownload]
tuxgt instance enable <game-id> <mod-id>
tuxgt instance disable <game-id> <mod-id>
tuxgt instance files <game-id> <mod-id>
tuxgt instance files <game-id> <mod-id> enable <dest> [--yes]
tuxgt instance files <game-id> <mod-id> disable <dest> [--yes]
tuxgt instance files <game-id> <mod-id> loaddll <dest>
tuxgt instance files <game-id> <mod-id> include <dest>
tuxgt cache refresh [<instance-id>]
tuxgt cache tools
```

Install line: `game<TAB>instance<TAB>files`. Unknown game/instance → error. Missing requires type → error naming the type (or install with `--with-requires`). Missing unpack tool → error naming the tool.

`instance files` list: `dest<TAB>loaddll|include<TAB>kept|omitted<TAB>required|optional`. Enable/disable then restage + prewire (skipped when the ini body is unchanged). Disable of required → error. Unknown dest → error. A keep-enable that would overwrite a foreign game-dir file confirms first (`--yes`, else a TTY prompt; non-TTY errors) and the check runs before any manifest or staging write, so a refused enable changes nothing.

Env keep: `tuxgt instance env <game-id> <mod-id>` lists `KEY<TAB>VALUE<TAB>kept|omitted`; `tuxgt instance env <game-id> <mod-id> enable|disable <KEY>` toggles one row (exact key match; unknown key → error).
