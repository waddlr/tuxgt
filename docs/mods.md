# Mods

A **Mod** is a recipe (a `.toml` file) that says where to get files and how to load them. A **Mods catalog** is the list of recipes TuxGT knows about. An **Instance** is that Mod installed for one game — a manifest plus staged files in `games/<…>/stage/` and `games/<…>/runtime/`. The **Game → Mods** tab shows Instances for the selected game; **Settings → Mods** shows the catalog.

## Mod types

Each recipe has a `type` field. The type decides the default destination, whether it needs ReShade, and which launch plan it can use.

**Shipped official recipes** — in the repo at `mods/official/`, carried in the tarball at `mods/official/*.toml` and after install at `$PREFIX/mods/official/*.toml`:

| File | `id` | `type` | What it does |
|------|------|--------|--------------|
| `optiscaler.toml` | `optiscaler` | `optiscaler` | OptiScaler — 64-bit games only. Latest GitHub `OptiScaler_*.7z` from `optiscaler/OptiScaler`. Preload keeps `OptiScaler.dll`. The Install prompt in the GUI shows a proxy name (`dxgi`, `d3d9`, `d3d10`, `d3d11`, `d3d12`, `winmm`, `version`) or `OptiScaler.dll` (the game will not load that stock-named DLL). `tuxgt instance install --slot` and `tuxgt instance slot` take `<self>` for that stock choice, or a proxy stem; the filename is not a slot token. Does not set `PROTON_USE_OPTISCALER`. TuxGT still offers and installs it on a 32-bit game ([known issue](../KNOWN_ISSUES.md)). |
| `optiscaler-y4my4m-v4.toml` | `optiscaler-y4my4m-v4` | `optiscaler` | y4my4m DLSS-NR multipass MFG fork, 64-bit games only. Pinned to the v4 pre-release `_with_DLSS.7z` (DLSS 310.9 + Streamline 2.14). Same stock-name preload and the same Install slot prompt as official OptiScaler. Does not set `PROTON_USE_OPTISCALER`. See [OptiScaler y4my4m fork](mods/optiscaler-y4my4m.md). |
| `reshade.toml` | `reshade` | `reshade` | ReShade 6.8.x — stock-named DLLs (`ReShade64.dll`/`ReShade32.dll`) on preload. The Install prompt in the GUI shows a proxy name or `ReShade64.dll` / `ReShade32.dll`. The CLI token for that stock choice is `<self>`, not the filename. Source is `manual_url` `https://reshade.me/downloads/ReShade_Setup_6.8.0_Addon.exe`. No shader files here. |
| `d3dcompiler-47.toml` | `d3dcompiler-47` | `custom` | Helper copy of Microsoft's `d3dcompiler_47.dll`. Pinned `sha256` is verified before install. Ships an env override `WINEDLLOVERRIDES=d3dcompiler_47=n` and allows the `preload` and `install` plans. |
| `nvngx-dlssnr.toml` | `nvngx-dlssnr` | `custom` | NVIDIA DLSS neural-rendering library. You provide `nvngx_dlssnr.dll`. TuxGT does not download it. Staged under its own name (not loaded as a proxy). |
| `nvngx-dlssnr-proxy.toml` | `nvngx-dlssnr-proxy` | `custom` | Workaround proxy for the neural-rendering library: proxy build plus renamed original. Use with ReShade DLSS-NR mods. |
| `renodx-dlss.toml` | `renodx-dlss` | `reshade_addon` | RenoDX DLSS addon. You provide the `renodx-dlss.addon64` file. TuxGT does not download it. Needs the ReShade, d3dcompiler_47, NVIDIA Streamline, and nvngx_dlssnr workaround-proxy mods for the same game. See [RenoDX DLSS](mods/renodx-dlss.md). |
| `deep-fried-chicken-64bit.toml` | `deep-fried-chicken-64bit` | `reshade_addon` | Deep Fried Chicken v3 for 64-bit games. Provide the DFC v3 7z (TuxGT prompts for the archive password when it is encrypted) or the package's `64-bit` folder. Needs the ReShade, d3dcompiler_47, NVIDIA Streamline, and nvngx_dlssnr workaround-proxy mods for the same game. See [Deep Fried Chicken](mods/deep-fried-chicken.md). |
| `nvidia-streamline.toml` | `nvidia-streamline` | `custom` | NVIDIA Streamline runtime (interposer, common, PCL, DLSS, frame generation, Reflex, and the three nvngx DLSS DLLs). You provide those files. TuxGT does not download it. Staged under their own names. |

No DLLs for OptiScaler, the y4my4m fork, ReShade, or `d3dcompiler-47` are inside the tarball — they are downloaded when you first install the Instance. `d3dcompiler-47` also shows up as a standalone mod users rarely enable directly. Other custom builds of OptiScaler and ReShade are supported — mint them from the `custom-optiscaler`/`custom-reshade` templates below.

**Templates for your own mods** — in the repo at `mods/templates/`, shipped at `$PREFIX/share/templates/*.toml` after install. Use them when you mint a user mod from a folder or archive (see "Add a custom pack" below). They set the type and the small rule that type implies:

| Template | `id` | `type` | Rule |
|----------|------|--------|------|
| `custom-reshade.toml` | `custom-reshade` | `reshade` | Same as official ReShade but from a local package you provide |
| `custom-optiscaler.toml` | `custom-optiscaler` | `optiscaler` | Same as official OptiScaler (64-bit games only) but from a local build or fork |
| `reshade-addon.toml` | `reshade-addon` | `reshade_addon` | An `.addon64` plus supporting files; `requires = ["reshade"]` |
| `reshade-shader.toml` | `reshade-shader` | `effect` | A shader pack (`.fx` files) under `reshade-shaders/Shaders`; `requires = ["reshade", "d3dcompiler-47"]` |
| `reshade-texture.toml` | `reshade-texture` | `texture` | A texture pack under `reshade-shaders/Textures`; `requires = ["reshade"]` |
| `custom-blank.toml` | `custom-blank` | `custom` | A plain DLL or loose files — no quirks, no slot pick unless the recipe declares a slot |
| `family-renodx.toml` | `family-renodx` | `reshade_addon` | Family: creates per-game HDR addons from `clshortfuse/renodx` releases `renodx-*.addon64` (prerelease allowed) |
| `family-luma.toml` | `family-luma` | `reshade_addon` | Family: creates per-game HDR addons from `Filoppi/Luma-Framework` zips `Luma-*.zip` (drops `dxgi.dll`) |

Notes: `reshade_addon`, `effect`, and `texture` always require ReShade — TuxGT prompts you to install it first and blocks if you decline. Families are not single Instances; each game that matches the family gets its own minted Instance (for example `renodx-cp2077` from the RenoDX family).

## Mods you provide

These official cards sit in the Official list. Until the files are present the right side is **Provide files** and no switch, and the missing names stay on the card. It takes one file (a DLL, an addon, or a zip/7z/rar) or a folder. A recipe that needs one file accepts any filename and stores it under the recipe's name. A recipe that needs several files matches each name, including a version tail such as `renodx-dlss-v1.addon64` for `renodx-dlss.addon64`. When every file is present that list hides, a clear button appears beside the switch, and the mod can be turned on. Until then it does not appear on the Game → Mods tab and cannot be installed.

Deep Fried Chicken ships as one password-protected 7z. Provide that 7z to the card. TuxGT asks for the password and uses the single folder that contains that card's files. When two folders each contain a full set, the card stays unresolved until you provide one of those folders. The card requires ReShade plus the d3dcompiler_47, NVIDIA Streamline, and workaround-proxy mods for the same game — installing offers them when missing. The `nvngx-dlssnr-proxy` card takes two files: your proxy build under its own name plus the real DLL renamed to `nvngx_dlssnr.real.dll`.

## Where mods live after install

```
$PREFIX/mods/official/<id>.toml        # shipped recipes (above)
$PREFIX/mods/official/<id>/            # payload: downloads and files you provide
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
2. Click that tab's Add button (**Add Custom OptiScaler**, **Add Custom ReShade** / **Add Custom Pack**, **Add Custom Mod…**). A file window opens — pick the folder, `.zip`/`.7z` archive, or single DLL as shipped (for example a shader zip with `Shaders/a.fx`, or a ReShade build with `ReShade64.dll`). The ReShade tab's **User Packs** header also offers **Add HDR Packs** (mint per-game RenoDX/Luma HDR addons from the live releases) and **Add ReShade Pack** (mint shader/addon packs from ReShade's live extras list, filtered All / Effects / Addons).
3. The preview lists the files found, drops known junk (families drop `dxgi.dll`; OptiScaler drops `*.bat`/`*.reg`), and lets you edit the mod id, label, Requires, dest paths, and which files are kept.
4. Save — the recipe is written to `$PREFIX/mods/user/<id>.toml` and the payload is copied to `$PREFIX/mods/user/<id>/`. It then appears in the catalog and on the Game → Mods tab (filtered to games it applies to).
5. Open a game, go to **Mods**, and **Install** the new mod for the game. Use the "N files" section on the installed card to toggle per-dest keep. A `.dll` row there (not a Proton prefix copy) also has a **Load** switch: on loads it (`LoadDLL`), off only stages it (`IncludeFile`). That choice is this game only. The GUI **Slot** control shows `dxgi`, `d3d9`, `d3d10`, `d3d11`, `d3d12`, `winmm`, `version`, or that mod's DLL name (`ReShade64.dll`, `ReShade32.dll`, `OptiScaler.dll`). Preload and Install each keep the slot last chosen for that mode. If a switch fails, the game stays on the mode it is on and that mode's files stay as they were; the other mode's slot is still offered the next time. `tuxgt instance slot` and `install --slot` take `<self>` for the stock name, not the filename. ReShade and OptiScaler always offer the control; a custom DLL only when its recipe declares a slot; addons and shaders never.

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

## Updates

TuxGT checks download-sourced mods on launch and about every two hours after. An installed card shows an **Update** button when a newer asset exists (the Settings catalog card also labels it **Update available**), or a short reason note (**Install record missing**, **Source unreachable**, **Files not provided**) with an **Update** retry — except **Files not provided**, which has no Update button until you **Provide files** again. Update re-downloads the payload and confirms first when edited configs would be overwritten.

## Edit mod configs

Text configs inside a mod (`.ini`, `.cfg`, `.conf`, `.toml`, `.json`, `.xml`, `.txt`) carry an **Edit** pen button: on a user mod it edits the shared payload copy, on an installed card it edits that game's staged copy. A full-page editor opens; **Open externally** hands the file to your editor and closes the page. Saving the payload copy re-pushes it to installed games that did not override it.

## Tips

- **Shader/texture packs** should use the `effect`/`texture` templates — the loader will put `.fx` under `reshade-shaders/Shaders/` and textures under `reshade-shaders/Textures`.
- **RenoDX/Luma HDR packs** are per-game. On the ReShade tab use **Add HDR Packs**, pick the game per row, and mint — each game gets its own user mod (for example `renodx-cp2077`).
- **Custom forks of OptiScaler or ReShade** — use the `custom-optiscaler`/`custom-reshade` templates so you do not shadow the official well-known ids. The y4my4m fork needs no template: it ships as an official mod, see [OptiScaler y4my4m fork](mods/optiscaler-y4my4m.md).
- **Keep is per game** — unchecking a file on one game's installed card hides it only for that game (for example optional companions of a mod). Required dests cannot be unchecked.
- **Load is per game** — the Load switch on an applicable `.dll` chooses `LoadDLL` or `IncludeFile` for that game. It does not change keep or the slot.
