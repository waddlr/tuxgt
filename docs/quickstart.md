# Quickstart

This is the shortest path from a fresh install to a game running with a mod. Nothing here needs the terminal, but terminal equivalents are noted where useful.

## 1) Open TuxGT

- After [Install](install.md) (`./tuxgt/bin/tuxgt install`), open TuxGT from your app menu — look for **TuxGT** under Games — or run `tuxgt` (or `tuxgt gui`) in a terminal.
- On first start you see the **Library** page.

## 2) Find your games

- TuxGT finds games from **Steam** and **Heroic** (Heroic covers GOG, Epic, and sideloaded games) automatically. You do not need to point it at folders.
- Standalone EXEs outside any store join from the terminal only: `tuxgt games add /path/to/game.exe` (remove one later with `tuxgt games remove <id>` or the trash on its Library card).
- Covers come from the stores plus SteamGridDB once a key is set: **Settings → Core Plugins → SteamGridDB** gear, paste the key, **Save** (use **Test** to check it).
- If a game is missing or you just installed a store, trigger a scan:
  - In the app: **Settings → General → Rescan libraries** (it re-runs detectors and refreshes the index).
  - In a terminal: `tuxgt scan` (and `tuxgt scan --force --yes` to re-run all detectors and clear overrides — it asks for confirmation without `--yes`).
- Use the search field at the top of the Library to filter by name. Hidden or disabled managers are filtered there too.

## 3) Open a game

- Click a game card in the Library. That opens the **Game** page.
- Along the top you see tabs — **General**, **Mods**, **Environment**. For this walkthrough, stay on **Mods**. (Per-game env options live on **Environment** — see [Env](env.md).)
- **General** also carries the rating pills, an **About** box (store IDs, Proton/Wine, paths), **Tools** (`winecfg`, `regedit`, `explorer`, `winetricks`), and an **Advanced** extras editor for loader exes Proton reports.

## 4) Enable a mod for that game

Mods are "recipes" — small text files that say where to fetch files and how to load them. The shipped recipes are 9 officials — OptiScaler (plus the y4my4m fork; 64-bit games only), ReShade, a helper DLL, NVIDIA Streamline, two nvngx_dlssnr variants, RenoDX DLSS, and Deep Fried Chicken — and templates let you add more (see [Mods](mods.md)).

1. On the Game → **Mods** tab you see the installed instances for this game (initially empty) and an **Install** button that opens the catalog picker (**Install mods** dialog).
2. Pick a mod to try — for example **OptiScaler** (64-bit upscaler) or **ReShade** (post-processing).
   - If the mod needs ReShade first (for example a ReShade addon, shader, or texture), the app will prompt "The following additional mods are required for your selected install:" and list the missing mods. Click **Install required** and it will set up ReShade first.
3. Click **Enable** (or **Install**) on the mod. If the content has not been downloaded yet, TuxGT fetches it now (you see a progress bar) and writes a manifest for this game. The first download may take a moment.
4. If two mods want the same DLL slot (for example two things that both want `dxgi.dll`), the app shows a slot conflict and lets you pick a different slot (`dxgi`, `d3d9`, `d3d10`, `d3d11`, `d3d12`, `winmm`, `version`) or use **Make win** / the move arrows in **Load conflicts** to decide which wins. You normally do not need to change this.
5. Optional: after installing, expand the "N files" section on the card to see which files will be staged and which dest DLL name was chosen.

If you picked ReShade, two extra steps before you Play:

- Add at least one shader pack (and any texture packs you want) — ReShade itself ships no `.fx` files, so with no packs the overlay opens but there is nothing to enable. Go to the **Settings → Mods → ReShade** tab, click **Add Custom Pack**, pick the shader zip, then **Save** (see [Mods](mods.md)); then install the new pack for the game.
- Install the `d3dcompiler_47` helper mod for the game too — ReShade needs the native compiler DLL to compile shaders (it ships a `WINEDLLOVERRIDES` override so Wine uses it instead of its builtin).

CLI equivalent for the same steps (same game id, same mod id — ids look like `steam::814380`):

```sh
tuxgt mods list steam::814380   # what mods are applicable
tuxgt instance install steam::814380 optiscaler
```

## 5) Choose how it loads

On the Game page, **Launch Mode** shows three choices (only the legal ones are enabled):

- **Hook protonfixes** — best when the game's Proton is GE or CachyOS. It does not write into the game folder and needs no store change.
- **Update Launch Options** — writes a trampoline wrapper into Steam's `LaunchOptions` or Heroic's wrapper options. Restart Steam/Heroic once after turning this on — both clients cache their config at startup. This is the mode that also supports outer wrappers like gamescope, GameMode, or MangoHud as argv wrappers (the hook cannot wrap those).
- **Not hooked** — no injection. Mods and knobs stay set up but are ignored — useful to test "does the game run vanilla?".

The app picks a sensible default: if you turn on an argv wrapper (gamescope / GameMode / MangoHud) while Hook is armed, it switches to Update Launch Options automatically. **Not hooked is always legal** — it pauses everything without uninstalling.

**Adapter** (same card) picks how files reach the game: **Preload** (default) stages files outside the game folder and injects via the launcher — the game folder stays untouched. **Install** copies proxy DLLs into the game folder instead — the fallback when preload misbehaves for a title. Switching converts every installed manifest for the game at once; if Steam/Heroic is running, TuxGT offers to stop it, convert, and restart it.

Once either hooked mode is set, you can close TuxGT — the app doesn't need to be running for mods to load from Steam/Heroic Play.

## 6) Press Play

- On the same Game page, press **Play**. That launches the game with the staged mods and any enabled env knobs — without changing the store's own Play button.
  - Library cards also have a **Play** button that does the same for that row without changing your selection.
- If you chose **Hook protonfixes**, the mod loads automatically on the next GE/Cachy launch.
- If you chose **Update Launch Options**, the first Play after enabling shows **Enable & Play** — that arms the mode (writes the trampoline into the store config), then launches. Restart Steam/Heroic after any Apply or Restore so the store picks up the new options. Afterwards, pressing **Play** inside Steam/Heroic itself also runs the trampoline.
- ReShade users: in-game, open the ReShade overlay (Home key) → **Settings**, and set **Effect Search Paths** and **Texture Search Paths** to the staged `reshade-shaders/Shaders` and `reshade-shaders/Textures` folders. The "N files" section on each installed card shows where each file lands under the game's runtime dir.

CLI equivalent (no store mutation): `tuxgt launch steam::814380` (add `--print` to preview the command, `--apply` to persist the trampoline, `--restore` to remove it).

## 7) After playing

- The next library scan harvests files: TuxGT records which files the game generated back into the manifest (so later Uninstall or verification can clean up). Run **Settings → General → Rescan libraries** (or `tuxgt scan`) before Uninstall if a game wrote new files.
- To turn a mod off for this game, reopen the Game → Mods card and click **Disable** (keeps files tracked) or **Uninstall** (removes staged files and the manifest).
- To change global settings (like the SteamGridDB key or Game Env defaults), go to **Settings**.
- Prefer no window? **Settings → General → Hide to Tray** hides on game launch, minimize, and/or close; the tray menu offers Show/Hide, Library/Settings jumps, recent Play, and Quit.

## Next

- Browse types and add a custom pack: [Mods](mods.md)
- Game did not appear or mod had no effect: [Troubleshooting](troubleshooting.md)
- Want to hack on TuxGT: [Build](build.md)
