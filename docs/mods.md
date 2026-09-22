# Mods

A **Mod** is a recipe (a `.toml` file) that says where to get files and how to load them. A **Mods catalog** is the list of recipes TuxGT knows about. An **Instance** is that Mod installed for one game — a manifest plus staged files in `games/<…>/stage/` and `games/<…>/runtime/`. The **Game → Mods** tab shows Instances for the selected game; **Settings → Mods** shows the catalog.

## Mod types

Each recipe has a `type` field. The type decides the default destination, whether it needs ReShade, and which launch plan it can use.

**Shipped official recipes** — in the repo at `mods/official/`, carried in the tarball at `mods/official/*.toml` and after install at `$PREFIX/mods/official/*.toml`:

| File | `id` | `type` | What it does |
|------|------|--------|--------------|
| `optiscaler.toml` | `optiscaler` | `optiscaler` | OptiScaler — latest GitHub `OptiScaler_*.7z` from `optiscaler/OptiScaler`. OptiScaler.dll is remapped to `dxgi.dll` when preloaded (type default). Also sets `PROTON_USE_OPTISCALER=1` when the game's Proton is CachyOS/GE. |
| `reshade.toml` | `reshade` | `reshade` | ReShade 6.8.x — stock-named DLLs (`ReShade64.dll`/`ReShade32.dll`). Source is `manual_url` `https://reshade.me/downloads/ReShade_Setup_6.8.0_Addon.exe`. No shader files here. |
| `d3dcompiler-47.toml` | `d3dcompiler-47` | `custom` | Helper copy of Microsoft's `d3dcompiler_47.dll`. Pinned `sha256` is verified before install. Ships an env override `WINEDLLOVERRIDES=d3dcompiler_47=n` and allows only the `preload` plan. |

No DLLs for those are inside the tarball — they are downloaded when you first install the Instance. `d3dcompiler-47` also shows up as a standalone mod users rarely enable directly. Custom builds of OptiScaler and ReShade are supported — mint them from the `custom-optiscaler`/`custom-reshade` templates below.

**Templates for your own mods** — in the repo at `mods/templates/`, shipped at `$PREFIX/share/templates/*.toml` after install. Use them when you mint a user mod from a folder or archive (see "Add a custom pack" below). They set the type and the small rule that type implies:

| Template | `id` | `type` | Rule |
|----------|------|--------|------|
| `custom-reshade.toml` | `custom-reshade` | `reshade` | Same as official ReShade but from a local package you provide |
| `custom-optiscaler.toml` | `custom-optiscaler` | `optiscaler` | Same as official OptiScaler but from a local build or fork |
| `reshade-addon.toml` | `reshade-addon` | `reshade_addon` | An `.addon64` plus supporting files; `requires = ["reshade"]` |
| `reshade-shader.toml` | `reshade-shader` | `effect` | A shader pack (`.fx` files) under `reshade-shaders/Shaders`; `requires = ["reshade"]` |
| `reshade-texture.toml` | `reshade-texture` | `texture` | A texture pack under `reshade-shaders/Textures`; `requires = ["reshade"]` |
| `custom-blank.toml` | `custom-blank` | `custom` | A plain DLL or loose files with a slot pick — no quirks |
| `family-renodx.toml` | `family-renodx` | `reshade_addon` | Family: creates per-game HDR addons from `clshortfuse/renodx` releases `renodx-*.addon64` (prerelease allowed) |
| `family-luma.toml` | `family-luma` | `reshade_addon` | Family: creates per-game HDR addons from `Filoppi/Luma-Framework` zips `Luma-*.zip` (drops `dxgi.dll`) |

Notes: `reshade_addon`, `effect`, and `texture` always require ReShade — TuxGT prompts you to install it first and blocks if you decline. Families are not single Instances; each game that matches the family gets its own minted Instance (for example `renodx-cp2077` from the RenoDX family).

## Where mods live after install

```
$PREFIX/mods/official/<id>.toml        # shipped recipes (above)
$PREFIX/mods/official/<id>/            # payload dir created on first download of that id
$PREFIX/mods/user/<id>.toml            # your own recipes
$PREFIX/mods/user/<id>/                # payload (copied on Add)
downloads/                             # in-flight fetch only; empty at rest
games/<l1>/<l2>/stage/<instance>/      # per-game staging (what gets prewired)
games/<l1>/<l2>/runtime/               # per-game loader dest (what the game sees)
games/<l1>/<l2>/manifests/<instance>.toml
```

`make prepare` and `make package` never copy payload dirs — only the recipes. `make deploy` syncs the official recipes into the live prefix: it writes/updates each shipped `*.toml` and deletes any official `*.toml` that left the package together with its payload dir; it never touches `mods/user/`, `games/`, `downloads/`, or payload dirs of still-shipped officials.

## Add a custom pack

You can add anything from a local folder, a zip/7z archive, or a single DLL. The tab you add from decides the type, which sets the dest root and requirements.

### In the GUI

1. Open **Settings → Mods** and pick the inner tab for the kind: **OptiScaler**, **ReShade**, or **Custom Mods**.
2. Click that tab's Add button (**Add Custom OptiScaler**, **Add Custom ReShade** / **Add Custom Pack**, **Add Custom Mod…**). A file window opens — pick the folder, `.zip`/`.7z` archive, or single DLL as shipped (for example a shader zip with `Shaders/a.fx`, or a ReShade build with `ReShade64.dll`).
3. The preview lists the files found, drops known junk (families drop `dxgi.dll`; OptiScaler drops `*.bat`/`*.reg`), and lets you edit the mod id, label, Requires, dest paths, and which files are kept.
4. Save — the recipe is written to `$PREFIX/mods/user/<id>.toml` and the payload is copied to `$PREFIX/mods/user/<id>/`. It then appears in the catalog and on the Game → Mods tab (filtered to games it applies to).
5. Open a game, go to **Mods**, and **Install** the new mod for the game. Use the "N files" section on the installed card to toggle per-dest keep, and **Slot** to pick a proxy slot when the type needs one (RenoDX/Luma/custom DLLs on `dxgi`, `d3d11`, `d3d12`, `winmm`, or `version`; stock-named ReShade and shaders need no slot).

### In the terminal

```sh
# From a local folder or archive — id and type are required:
tuxgt mods add-from --type reshade_addon --id my-addon --path /path/to/folderOr.zip

# Or: add a pre-written recipe TOML directly:
tuxgt mods add /path/to/my-mod.toml

# Then install for one game (id looks like steam::814380):
tuxgt mods list steam::814380   # filter catalog to this game
tuxgt instance install steam::814380 my-addon
tuxgt instance status steam::814380  # per-file staging sync state
tuxgt instance files steam::814380 my-addon  # list/toggle per-dest keep
tuxgt instance slot steam::814380 my-addon dxgi
```

Other useful commands: `tuxgt mods list`, `tuxgt mods enable <id>` / `disable`, `tuxgt mods rescan <id> --yes` (refresh a user mod's local package), `tuxgt mods remove <id>`, `tuxgt mods export <id> --out recipe.toml [--files]`.

## Tips

- **Shader/texture packs** should use the `effect`/`texture` templates — the loader will put `.fx` under `reshade-shaders/Shaders/` and textures under `reshade-shaders/Textures`.
- **RenodX/Luma HDR packs** are per-game. Add the `family-renodx` or `family-luma` template once; then each matching game can install its own minted entry (match is via the `games` globs and `appids` overlay in the recipe).
- **Custom forks of OptiScaler or ReShade** — use the `custom-optiscaler`/`custom-reshade` templates so you do not shadow the official well-known ids. Worked example: [Custom OptiScaler build](custom-optiscaler.md).
- **Keep is per game** — unchecking a file on one game's installed card hides it only for that game (for example optional companions of a mod). Required dests cannot be unchecked.
