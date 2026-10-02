## Mods + recipes

Fields may gain; not an external ABI.

Vocabulary: a **Mod** is a definition (this recipe); an **Instance** is its per-game runtime (FileManifest + staging, `download.md`). Settings → Mods lists Mods; the Game Mods tab lists Instances. Install picks a Mod and creates an Instance.

Not in this surface:

| Item | Where |
|---|---|
| Downloading / cache layout | `download.md` |
| FileManifest, enable/install | `download.md` |
| GUI Mod catalog / add / remove / enable | `docs/dev/app/gui/settings.md` (Settings Mods page) |

### Recipe schema

Keys match the `docs/dev/app/core/mods.md` instance blob. The source discriminant is `type`, not `kind`.

```toml
id = "optiscaler-xyz"        # slug; see below
type = "optiscaler"          # ModType name; must parse
label = "OptiScaler fork"
plans_allowed = ["preload", "install"]   # optional; default ["install", "preload"]
games = ["*Ace Combat 7*"]   # optional; display-name globs
appids = [1245620]          # optional; nonzero u32 Steam AppIDs
requires = ["reshade"]     # optional; other Mod ids; install needs each one to have an Instance on that game
include = ["foo.dll"]      # optional; default IncludeFile even for a .dll dest (per-game load can switch it)
slot = "dxgi"              # optional; inferred from Remap when the stem parses
shader_dir = "OtisFX"      # optional; extra dest component under reshade-shaders/Shaders
texture_dir = "OtisFX"     # optional; extra dest component under reshade-shaders/Textures
effect_files = ["qUINT_sharp.fx"]  # optional; EffectFiles the recipe was minted from (display only)

[[payload]]               # optional, repeatable; absent = keep every extracted file
arch = "64"               # optional gate: game bitness (32 | 64)
api = "dx12"              # optional gate: game api
keep = ["ReShade64.dll"]  # globs kept when every declared gate matches; unknown arch/api uses the union of every keep glob
drop = ["*.json"]         # globs dropped when every declared gate matches

[source]
type = "github"              # github | local | manual_url | provided
owner = "..."
repo = "..."
asset_glob = "..."
tag = "..."                  # optional; release tag; absent = latest non-prerelease
prerelease = true            # optional; default false; github only
```

`appids` entries must be nonzero; empty is allowed. `prerelease = true` makes the source follow the **newest non-draft release including prereleases** (`GET /releases`, nightly repos 404 on `/releases/latest`). It is a github-source key — a `local`/`manual_url` recipe carrying it errors — and it is rejected combined with a pinned `tag` or a recipe-level `sha256` (follow-latest contradicts a pin; a vanished pin would 404). Resolution: `tag` → pinned `/releases/download/<tag>/<asset>` (glob assets via the tag's release API); no `tag`, no `prerelease` → literal `/releases/latest/download/<asset>` or `/releases/latest` API glob match; `prerelease = true` → newest non-draft release, then the same glob pick. A vanished pinned tag → 404 naming re-mint from the family.

`local`: `source.path`. `manual_url`: `source.url`. `provided`: `source.files` (non-empty relative globs, one required file each; no absolute path, no `..`) and optional `source.note` (omitted → empty; a present note is non-empty and at most 400 chars). `owner`, `repo`, `asset_glob`, `tag`, `prerelease`, `path`, and `url` on a `provided` source are a parse error. Recipe `sha256` is legal only when `files` has exactly one pattern. TuxGT does not download this source. Settings → Mods **Provide files**, or `tuxgt mods provide <id> --path <file|archive|dir>`, copies unique matches into the payload under the pattern's name, with glob marks removed (`renodx-dlss*.addon64` is stored as `renodx-dlss.addon64`). A one-file recipe accepts the single supplied file whatever its name. Several files match a basename glob or a same-extension prefix (`renodx-dlss-v1.addon64` for `renodx-dlss.addon64`; a `-` or `.` tail, not `_`, so `sl.dlss_g.dll` does not fill `sl.dlss.dll`). `x64` wins over `development` / `x86` / `win32`. A matched or unmatched drop does not raise an error. Until every pattern matches exactly one payload file, `mods_for_game` omits the mod and `instance install` errors with the missing or ambiguous names and writes nothing. A complete payload installs from disk. Unknown `source.type`, unknown fields, unknown mod `type` (incl. reserved `env_tool` and `custom_dll`), empty `plans_allowed`, bad payload `arch`, a plan outside `install`/`preload`/`proton_env`, an empty `games` entry, an `include` entry outside the payload, a `slot` that does not parse, a `tag` that is empty or outside `[A-Za-z0-9._-]`, a `shader_dir` / `texture_dir` that is empty or contains `/` or `\`, or an `effect_files` entry that is empty or contains `/` or `\` → error. Omitted `shader_dir` / `texture_dir` = no extra dest component (`dest_for`). Non-`effect` / `texture` recipes carrying either key → error.

Optional `effect_files`: the `EffectFiles` names the recipe was minted from. Display only — it never gates payload, dests or install; the GUI `Effects` preview paints it with the recipe's own `drop` globs applied. Hand-written recipes and Mods without the key have none.

Optional `[dests]`: map of archive-relative src → dest. Applied after payload keep/drop, before type `dest_root`. Explicit dests win over type dest rules (`OptiScaler.dll` → `dxgi.dll`). A dest may use the `pfx:windows/system32/<file>` / `pfx:windows/syswow64/<file>` form to land in the game prefix. Missing table → type rules only.

Optional `[env]`: string table of extra env this recipe contributes when installed (`WINEDLLOVERRIDES = "d3dcompiler_47=n"`). Keys validate like custom-env keys; empty keys/values → error. Install snapshots it into the manifest `[[env]]` (per-game keep bits; same preserve/default/drop rules as dests). Launch merges it after `env_custom` (generic last-wins, `WINEDLLOVERRIDES` stem-merge — see `launch-adapter.md`). Recipe env is never an EnvKnob and never global.

Payload (which extracted files enter the manifest): matching keeps win; if some rule declares `keep` but none of the matching rules do, use the union of every keep glob (gates ignored); if no rule declares `keep`, keep everything; then subtract union of matching `drop`s. Unknown game arch/api matches no gate. Rules that keep nothing for a game fail the install. Built-in repo junk always drops regardless of rules: dot-leading path segments, `_config.yml`, `README` with a `.`/`-`/`_` or end-of-name boundary (any case), and a file exactly named `dummy` (effect-junk-dests; existing installs shed them on reinstall).

### Catalog index

SQLite `mods_cache` table (one row per catalog Mod id; files stay source of truth).
`installed` counts FileManifests naming the id; `asset_sha256` is the catalog
payload version from `.provenance.toml` (empty until the first acquire).
`reconcile_mod_cache` runs at `open_db` (insert/delete/recount) and is the
only writer of the derived columns. A fresh open always rebuilds; a shared-pool
hit rebuilds only when a mutator marked the cache dirty — manifest and recipe
writers mark on success — or the 60s backstop elapsed for hand-placed files.
The poll always opens first, so it reads fresh rows after any marked mutation.

### Per-game applicability

`games` holds case-insensitive globs matched against the game **display name** (`*`, `?`; no `[` class support — the shared matcher has none). `appids` holds nonzero Steam AppIDs matched against the game's resolved AppID (stored overlay wins, else the Steam segment; unknown/manual without either → none). Match rule: a recipe with no `games` **and** no `appids` applies to every game; otherwise it applies when a `games` glob matches the display name **or** any stored `appid` equals the resolved AppID. `mods_for_game(config_dir, game_name, steam_appid, data_dir)` returns the catalog filtered to applicable **and enabled** Mods (broken user files still list as problems); `list_mods` keeps returning all, with enabled. CLI: `tuxgt mods list [<game-id>]` resolves the name + AppID from the game row and filters (unknown id → error, same line format). The Game Mods tab uses `mods_for_game`; Settings → Mods keeps the full catalog.

Official per-game RenoDX/Luma rows are **not** first-party: all eleven rows live in `mods/contrib/` (not deployed — copy into `$PREFIX/mods/official/` by hand to test) and every title mints a **user** Mod from an HDR-packs template. Do not add more official per-game Mods.

### Official catalog

Official `optiscaler` `v0.9.4` archive `Optiscaler_0.9.4-final.20260718._MM.7z` has one `OptiScaler.dll` at archive root.

| id | type | source | asset | games |
|---|---|---|---|---|
| `reshade` | `reshade` | reshade.me `manual_url` (`ReShade_Setup_6.8.0_Addon.exe` — full add-on support, unsigned; arch-gated keep of `ReShade64.dll` / `ReShade32.dll`) | any |
| `optiscaler` | `optiscaler` | `optiscaler/OptiScaler` | `OptiScaler_*.7z` (latest non-prerelease; `OptiScaler_0.9.4-final.20260718._MM.7z`) | any |
| `optiscaler-y4my4m-v4` | `optiscaler` | `y4my4my4m/OptiScaler_DLSSNR_Multipass_MFG` | `OptiScaler_v10.0.0-dev-fork-y4my4my4m-v4_20260905_with_DLSS.7z` (pinned tag `v10.0.0-dev-fork-y4my4my4m-v4`, pre-release v4; `OptiScaler.dll` at root, `OptiScaler.ini` + NR shim kept, Licenses/dev/setup files dropped) | any |
| `d3dcompiler-47` | `custom` | msdl.microsoft.com `manual_url` (`d3dcompiler_47.dll` as `LoadDLL` on preload, game-dir copy on install) + `[env]` `WINEDLLOVERRIDES=d3dcompiler_47=n` | any |
| `nvngx-dlssnr` | `custom` | `provided` — user supplies `nvngx_dlssnr.dll`. TuxGT does not download it. `include` stages it as IncludeFile | `nvngx_dlssnr.dll` | any |
| `nvngx-dlssnr-proxy` | `custom` | `provided` — user supplies the proxy build as `nvngx_dlssnr.dll` plus the real DLL renamed to `nvngx_dlssnr.real.dll`. Alternative to `nvngx-dlssnr`; both stage `nvngx_dlssnr.dll`, so load order decides | those two basenames | any |
| `renodx-dlss` | `reshade_addon` | `provided` — user supplies `renodx-dlss*.addon64`. TuxGT does not download it. Requires ReShade | `renodx-dlss*.addon64` | any |
| `deep-fried-chicken-64bit` | `reshade_addon` | `provided` — user extracts the v3 7z and supplies the `64-bit` folder (4 files). Requires ReShade | addon64 + cfg + 2 DLLs | any |
| `deep-fried-chicken-32bit` | `reshade_addon` | `provided` — user extracts the v3 7z and supplies the `32-bit` folder (8 files); `[dests]` restores `host64/` + `reshade-shaders/Shaders/` incl. DFC `ReShade.fxh`. Requires ReShade | addon32 + bridge cfg + host64 tree + feed fx | any |
| `nvidia-streamline` | `custom` | `provided` — user supplies the nine x64 runtime DLLs (`sl.interposer.dll`, `sl.common.dll`, `sl.pcl.dll`, `sl.dlss.dll`, `sl.dlss_g.dll`, `sl.reflex.dll`, `nvngx_dlss.dll`, `nvngx_dlssd.dll`, `nvngx_dlssg.dll`). TuxGT does not download it. Each name is `include`, so they stage as IncludeFile and `sl.interposer.dll` is not renamed to `dxgi.dll` | those nine basenames | any |

The eleven per-game rows (`renodx-baldurs-gate-3`, `renodx-cyberpunk-2077`, `renodx-final-fantasy-xvi`, `renodx-doom-eternal`, `renodx-the-witcher-3`, `luma-metaphor`, `luma-nier-automata`, `luma-monster-hunter-world`, `fxshaders`, `reshade-hdr-shaders`, `shadertoggler`) live in `mods/contrib/`: not packaged by `make package`/`make deploy` — copy one into `$PREFIX/mods/official/` by hand to test, or mint a user Mod from the HDR-packs templates / ReShade extras instead. `fxshaders` / `reshade-hdr-shaders` / `shadertoggler` mint from the extras lists (rolling master / pinned `1.2.2` / addon-only-keep differences apply); `renodx-cyberpunk-2077` mints from the RenoDX HDR pack.

RenoDX publishes only pre-releases (nightlies and `snapshot`), so `/releases/latest` 404s. The `family-renodx` template (and the `renodx-cyberpunk-2077` row) follow the newest non-draft nightly (`prerelease = true`); upstream rolls/retires tags, so a hand-pinned `tag` 404s once replaced — re-mint or re-pin. The `shadertoggler` row keeps the addon only. Luma zips ship several top-level entries (`Luma/`, `dxgi.dll`, `nvngx_dlss.dll`, `Luma-<Title>.addon`), so unpack keeps the tree as-is; payload drops only the bundled `dxgi.dll` (stock crosire ReShade 6.8.0.1), which would collide with our managed ReShade preload and OptiScaler's dxgi slot. The rest of the upstream tree (`nvngx_dlss.dll` DLSS override, `Luma/d3dcompiler_47.dll` compiler hook, `Luma/**` shaders, `Luma-<Title>.addon`) is kept; `install.copies` backs up any pre-existing game-dir file before overwriting. The addon is loaded by ReShade (x64 loads both `.addon` and `.addon64`, `addon_manager.cpp`).

No `texture` recipe ships: no credible GitHub-release texture source exists (ReShade texture packs live on Nexus/manual sites). The `texture` type is wired and ready for a user recipe.

Literal asset names are pinned; upstream renames need a recipe bump. Official `optiscaler` is the exception: glob stays (latest stable), case-insensitive. Recipe `sha256` is intentional only on `d3dcompiler-47`. Official `reshade` and `optiscaler` track latest and stay unpinned.

Official `optiscaler` dest: archive basename `OptiScaler.dll` → `dxgi.dll` (type `default_slot`). Zero or two such files → install error. Drop `*.bat` / `*.reg` / `*.md` / `*README*.txt` (Linux). The custom v3 tree in `external/` is not this Mod; a local/custom build is a **user Mod** (`instance.from-package`) with a different id. Official `optiscaler-y4my4m-v4` shares the type dest; its `OptiScaler/` tree and NR shim stage as companions.

`requires` lists other Mod ids: `d3dcompiler-47` (Provides `custom`, which requires nothing) carries no enforced dependency — docs guidance is use with the `reshade` Mod.

### Identity

Mod ids are a separate namespace from plugin ids. Slug: `[a-z][a-z0-9-]{0,31}`. A user recipe whose id equals an official id (`reshade`, `optiscaler`) is rejected — no shadowing. A custom build takes its own id with the same `type` (e.g. `id = "reshade-custom"`, `type = "reshade"`).

### Locations

Official Mods ship as packaged TOML: repo `mods/official/*.toml`, live `$PREFIX/mods/official/<id>.toml` — no `OFFICIAL_*` literals in the binary. Payload lands beside the recipe at `$PREFIX/mods/official/<id>/` on first install (not in the tarball). User Mods live as `$PREFIX/mods/user/<id>.toml` plus sibling `$PREFIX/mods/user/<id>/`. `TUXGT_CONFIG` does not relocate user recipes; they follow `data_dir()`. Listing merges officials first, then user files, then other first-level dirs whose names pass `[a-z][a-z0-9-]{0,31}` (`official` and `user` reserved). Registry writers are Later (`gui.plugin-registry`); a registry file whose id equals an official or user id is a problem entry. Officials win; user cannot shadow official. The app may cache parsed rows in sqlx for speed, keyed by file sha256; on every process start a new, changed, or gone packaged file reloads (a gone file drops the id, no ghost). **Re-sync official** on Settings → Mods reloads without restart.

A bad user file never breaks the list: it becomes a problem entry (`file` + `reason`) while the rest still lists. A user file claiming an official id is ignored the same way — officials win; nothing is silently replaced.

### Enable / disable

Official Mods cannot be removed. They **can** be disabled. User Mods can be disabled or removed.

Source of truth: `<config>/mods.toml` (`TUXGT_CONFIG`, else `$PREFIX/config`). Not `plugins.toml` (plugin ids are a different namespace). Not sqlx. Not the recipe file.

```
disabled = ["reshade"]
```

Missing file or missing id ⇒ enabled. File is created only on `enable`/`disable`. Ids in `disabled` that are not in the loaded table are kept on disk and not listed.

- Disable: still listed on Settings → Mods; `enabled = false`. Later surfaces must not offer that Mod (`mods_for_game`, Game Mods tab). `install_instance` of a disabled id errors (`InstanceDisabled`), including `--with-requires`. Per-game enable/uninstall of an existing FileManifest still works.
- Do not delete recipes, manifests, downloads, or per-game FileManifests. Re-enable restores the offer; already-installed Instances stay until the user uninstalls. No cascade uninstall.
- Default: every listed Mod enabled, including official.
- Default-Mod / “install default of this kind” skips disabled. Zero enabled Mods of a required kind → missing-require, not a silent re-enable of official.

```
tuxgt mods enable <id>
tuxgt mods disable <id>
```

Unknown id (not in the loaded table) → error. Idempotent if already in the requested state. Official id is valid here; `remove` of an official id still errors.

### From package + classify

A user Mod of an existing Provides kind from a local directory or archive. Does not shadow official ids. GUI: Settings → Mods, Add + Template (`docs/dev/app/gui/settings.md`). Host API is the product surface.

Scan lists archive-relative paths (dir walk, or unpack-to-temp; single top-level dir stripped). Default keep = all files minus type drops; dests = type dest rules. Extra files vs official are kept. From-package writes an explicit `keep` list (later files on disk are not installed until rescan) plus `[dests]` where dest ≠ src.

Classify (`classify_package`): a file that is a `.toml` that parses is a Mod recipe; an archive unpacks to temp, then classifies as a folder; `.dll` / `.addon` / `.addon64` is a Single injectable; anything else is a Single regular file. A directory with exactly one file classifies as that file; with exactly one parseable `*.toml` it imports as a recipe with `source` rewritten local; otherwise it is Multi-file. Several valid recipe TOMLs → error; archive cancel deletes the temp.

Every non-download add copies its payload into `$PREFIX/mods/user/<id>/` on Save; the recipe has `source.type = "local"` and `source.path` = that copy (or the copied file). Archives unpack under `/tmp/tuxgt-<hash>/` then land; tmp is deleted after. Install, rescan, and dest remap read only the copy — deleting or moving the picked file after Save is fine. Cancel leaves no payload dir behind. Github / `manual_url` recipes acquire at first install (no copy at Add); a later install of the same id skips acquire when the payload dir already has files unless `--redownload`. A picked `.toml` that is only a remote recipe imports the recipe with no payload copy. Rescan walks the copy, never the original.

```
tuxgt mods add-from --type <modtype> --id <slug> --path <dir-or-archive> [--label <str>] [--yes]
tuxgt mods rescan <id> [--yes]
```

`--yes` required when not a TTY (keeps scan defaults). `add-from` writes `$PREFIX/mods/user/<id>.toml` (`source.type = "local"`). `rescan` user Mods only; official → error. Rescan does not rewrite per-game FileManifests; next `instance install` / reinstall does. Rescan rewrites `include` from its caller: CLI `mods rescan` passes none and keeps the recipe's list; the GUI Rescan form seeds the kept-DLL `Load | Include` modes from `include` and saves the form's include dests.

Existing `add <recipe-file>` still imports a hand-written TOML.

### Instance slot

`tuxgt instance slot <game> <mod> <slot> [--yes]` rewrites the claiming Load dest's basename to the proxy slot stem (`dxgi`, `d3d9`, `d3d10`, `d3d11`, `d3d12`, `winmm`, `version`; a `.dll` suffix is optional), then restages and prewires; prints `game<TAB>instance<TAB>slot`. The claiming dest is the enabled Load DLL (an `include`-covered DLL never claims); when several Load DLLs exist the slot-named one wins, otherwise a lone Load DLL. Types with no claiming dest (stock-named ReShade, addons, shaders) error; the same slot is a no-op. Reinstall keeps the renamed dest: the preserve pass matches prior dests **by source**, so a `winmm.dll` rename survives reinstall while a new source takes the computed dest. Install-adapter foreign game-dir dests confirm first (`--yes`, else a TTY prompt; non-TTY errors). GUI: the Add-dialog Slot picker and the Game installed-card Slot dropdown (`docs/dev/app/gui/settings.md`, `docs/dev/app/gui/game.md`).

### Templates

A Template is a pre-filled Mod (Provides, Mode, dest rules, Requires) or a family source. One mechanism, packaged TOML: repo `mods/templates/*.toml`, live `$PREFIX/share/templates/*.toml`. Same recipe schema as Mods; a recipe whose Provides mismatches the template errors.

| Template | Provides | Mode / dest | Requires | Form |
|---|---|---|---|---|
| ReShade Addon | `reshade_addon` | Include File, dest basename | Mod that Provides `reshade` | Name + File |
| ReShade Shader | `effect` | Include File, `reshade-shaders/Shaders` | ReShade | Name + File |
| ReShade Texture | `texture` | Include File, `reshade-shaders/Textures` | ReShade | Name + File |
| OptiScaler | `optiscaler` | type dests | — | Name + File/Folder |
| ReShade | `reshade` | type dests | — | Name + File/Folder |
| Custom (blank) | `custom` | classify form | user picks | full form |

Single custom Add infers the template: scan first; `effect` unless a `.addon64`/`.addon` file is present (then `reshade_addon`). A mixed addon+shader pack mints as `reshade_addon` with the `.fx` files kept archive-relative (same rule as extras-mint addon leftovers). All three pack templates stay prefill sources for the inferred type.
Family templates carry an optional `[family]` table instead of a form mode — the per-tab prefill find skips them. User label: **HDR packs**.

```toml
id = "family-renodx"          # family-luma: Filoppi/Luma-Framework, Luma-*.zip, no prerelease, drop ["dxgi.dll"]
label = "RenoDX"
type = "reshade_addon"

[family]
owner = "clshortfuse"
repo = "renodx"
asset_glob = "renodx-*.addon64"   # filters the listed assets (case-insensitive)
prerelease = true                 # optional; default false
drop = []                         # optional payload drop globs written into minted recipes
```

Mint (`list_family_assets_many` / `family_template` + `snap_family_asset` + `RecipeSpec::family` + `mint_recipe`, `gui.family-mint`): single `Add HDR Packs` flow merges both families' live releases (resolved prerelease-aware exactly as an install would; a failed vendor still lists the other with its error shown on the card, each row badged RenoDX/Luma), assets matching `family.asset_glob` (case-insensitive) listing with the release tag. Each row auto-matches a library game (vendor prefix stripped, then exact token equality — one fused alpha/digit half of len >= 3, e.g. `2077` in `cp2077`, may exact-match instead; single hit pre-checks, zero/several stay unchecked for user correction; picking a game checks the row); the per-row game dropdown binds the pick — row with a resolved AppID writes `appids = [id]` + no `games`, row with a game lacking one writes `games = ["*<display>*"]` + no `appids`, row with no game is skipped and named in the toast. Label is always auto (stem minus the family `label-` prefix, `-`/`_` → space). Minting derives the id slug from the asset stem with the extras sanitizer (`package_slug`: lowercase, space/`_` → `-`, strip the rest, 32-col; over-long stems keep trailing `test`/`dev`/`x32` tokens, truncating the base to fit), then appends `-2`/`-3`/… only on residual collision with a listed id (same-asset re-mint is still refused as already-in-catalog, never renamed), and writes `$PREFIX/mods/user/<id>.toml`: github source + the picked **literal** asset name + inherited `prerelease` (no tag); `[payload] drop` from `family.drop` only when non-empty; mode is the kind's single-file addon flow (Include File, dest basename; no explicit `requires`, matching the demoted rows). No manual asset/AppID/Title entry; no future-game (not-in-library) mint. Stop on first error; toast lists minted vs skipped. Families are data, not code: a third family is one new TOML. CLI `tuxgt mods mint list-family` / `tuxgt mods mint family --template ID --asset NAME --game ID`.

### ReShade extras (`instance.reshade-packages`)

Crosire’s Windows Setup with addon support offers a checkbox list of extra shader/texture packs and addons after installing the DLL. The equivalent here is **catalog mint**, same shape as Family, not an install-time wizard and not more official TOMLs.

Live lists (GET, revalidate, download `USER_AGENT`, no cache):

- `https://raw.githubusercontent.com/crosire/reshade-shaders/list/EffectPackages.ini`
- `https://raw.githubusercontent.com/crosire/reshade-shaders/list/Addons.ini`

`list_reshade_packages` parses both (small INI parser, no crate). Skip `#` comment sections. URL parsing: `DownloadUrl`/`DownloadUrl64`/`DownloadUrl32` all parsed; `ReshadePackage` stores `url` (64-bit: `DownloadUrl64` else `DownloadUrl`) and `url32` (`DownloadUrl32` when present). Row with no URL is listed but not mintable (RenoDX, PyHook, Geo3D, `reshade_cv`). `url_for_arch("32")` is strict — `url32` or `None`, never the 64-bit `url`. The catalog lock is per-arch (`package_in_catalog_for_arch`): a listed 32-bit recipe locks only the 32-bit variant and vice versa; `key()` stays `kind:name`. Mints are per-game via `RecipeSpec::reshade_package_for_game(pkg, arch, target)` (pub): the Settings → *ReShade extras* card requires a target game, reads its effective bitness (`GameRow.bitness`: override else detected), and mints the one matching variant. A 32-bit target with no `DownloadUrl32`, or no target at all, fails closed with no recipe written. `Compatibility.ini` is not this surface.

`RecipeSpec::reshade_package_for_game` + `mint_recipe` writes `$PREFIX/mods/user/<id>.toml` from the listed snapshot (no second GET). Identity is per-arch: base slug `B` from `PackageName` (lowercase, space/`_` → `-`, `[a-z][a-z0-9-]{0,31}`), then `B-x32` / `B-x64` with the suffix applied before the 32-column truncation, so both variants coexist and neither shadows the other. The recipe binds the target game — resolved AppID → `appids = [id]`, else `games = ["*<display>*"]`. Shadowing a listed id, or an already-in-catalog variant for that arch, → error, no write. Label = `PackageName`. CLI `tuxgt mods mint list-reshade` / `tuxgt mods mint reshade --package NAME --game ID [--kind effect|addon]`; GUI calls core (`gui.reshade-packages`). Migration: before a package mints, its existing un-suffixed 64-bit recipe (id `B` whose source matches the package URL) renames to `B-x64` — recipe file, sibling payload dir, `mods.toml` entry, every manifest with `instance == B`, every user-recipe `requires` entry `== B`. If `B-x64` is taken anywhere, or the match is ambiguous, all files stay unchanged and the mint reports the conflict.

| INI | Minted recipe |
|---|---|
| EffectPackages.ini | `type = "effect"` (one Mod; `dest_for` already splits `Shaders/` and `Textures/`) |
| Addons.ini | `type = "reshade_addon"` |
| URL matching `/releases/latest/download/<asset>` or `/releases/download/<tag>/<asset>` | `source.type = "github"` |
| anything else (branch archives, odd zips) | `source.type = "manual_url"` (stable URL; `check_update` will not flip Available; `--redownload` still works) |
| `DenyEffectFiles` | `[[payload]] drop` globs matching basename (`Template.fx` and `*/Template.fx`) |
| `EffectFiles` | recipe `effect_files` (display only; the drops above still hide the denied names in the preview) |
| `InstallPath` last component unless `Shaders` | `shader_dir` |
| `TextureInstallPath` last component unless `Textures` | `texture_dir` |
Already in catalog (per arch): github `owner/repo` from `RepositoryUrl` / the arch's `DownloadUrl` matches a listed Mod's github source for that arch's asset, or that arch's `manual_url` equals a listed `source.url` (covers user-minted `fxshaders`, `reshade-hdr-shaders`, `shadertoggler`). A locked variant is not mintable again; the other arch stays mintable. A listed un-suffixed base id never locks; minting migrates it to `B-x64` first. RenoDX with no URL stays HDR-packs mint.

Addon extra `.fx` (`EffectInstallPath`): leftover non-addon files stay archive-relative `IncludeFile` trees (Luma). Do not write `ReShade.ini` `EffectSearchPaths`. `EffectInstallPath` is display-only.

Do not overload `[family]` templates. Do not ship the INI packs as `mods/official/*.toml`.

### `proton_env`

`proton_env` (`PROTON_USE_OPTISCALER=1`) is official-only. A user Mod listing it is rejected at add time. Only official packaged Mods may carry it; the check is origin, not content. Shipped `optiscaler` and `optiscaler-y4my4m-v4` do not list it. A manual OptiScaler install does not set `PROTON_USE_OPTISCALER`.

### Requires semantics

Install enforces both the Provides-kind Requires from `mod-type.md` and per-recipe `requires` Mod ids: each required Mod must have an Instance on that game, or `MissingRequires`. A custom `reshade` Mod satisfies a kind-level `reshade` require. No silent dep enable.

### Source resolve stub

```
resolve_source(instance) -> SourceRef   # Github{...} | Local{...} | ManualUrl{...} | Provided{files, note}; parse only, no I/O
```

### CLI

```
tuxgt mods list [<game-id>]
tuxgt mods add <recipe-file>
tuxgt mods add-from --type <modtype> --id <slug> --path <dir-or-archive> [--label <str>] [--yes]
tuxgt mods rescan <id> [--yes]
tuxgt mods remove <id>
tuxgt mods enable <id>
tuxgt mods disable <id>
tuxgt mods provide <id> --path <file-or-archive-or-dir>
tuxgt mods mint list-family
tuxgt mods mint list-reshade
tuxgt mods mint family --template <id> --asset <name> --game <id>
tuxgt mods mint reshade --package <name> --game <id> [--kind effect|addon]
```

List line: `id<TAB>type<TAB>label<TAB>source-type<TAB>plans<TAB>enabled|disabled`. Officials carry no origin marker in v1 output. Broken files print `broken<TAB>file<TAB>reason` to stderr; the list still exits 0. Add validates then writes `<id>.toml`; remove deletes it. Removing an official id → error. Unknown id on remove/enable/disable → error. `mods provide` is only for `source.type = "provided"`: it prints `present` / `missing` / `ambiguous` lines, stores every unique match even when the set is short, and exits non-zero until every pattern matches exactly one payload file. Completing the set enables the catalog entry. A later provide of one file replaces that payload file and leaves an already-enabled mod enabled.

### Export

User Mods only; official and registry ids error, unknown ids error.

```
tuxgt mods export <id> --out <path> [--files]
```

Recipe-only (no `--files`) writes `path` as TOML: remote sources as stored; a `provided` source is stored as `files` + `note` with no bytes; a local `source.path` is rewritten relative to the prefix (no absolute leak). Recipe + files (`--files`) writes a `.tar.gz` holding `<id>.toml` plus the kept payload srcs under `payload/`; the bundled recipe points at `payload`, so extracting the archive and running `tuxgt mods add <id>.toml` re-imports through the usual recipe classify + `add_mod` path. A remote source, including `provided`, with `--files` errors (no local source path to bundle). Archiving shells out to PATH `tar`; a missing binary is a `MissingTool("tar")` naming error.

