# Custom OptiScaler build

Walkthrough for running a third-party OptiScaler build, using the
[y4my4my4m DLSS-NR multipass MFG fork](https://github.com/y4my4my4m/OptiScaler_DLSSNR_Multipass_MFG)
as the example. (Pre-release v4 at the time of writing; verified upstream under Proton.)

## 1. Download the fork

From the fork's [Releases](https://github.com/y4my4my4m/OptiScaler_DLSSNR_Multipass_MFG/releases),
download the latest pre-release **with the DLSS package** (`..._with_DLSS.7z`) — it bundles
DLSS 310.9 (`nvngx_dlss.dll`, `nvngx_dlssd.dll`, `nvngx_dlssg.dll`) and Streamline 2.14, which the
plain archive omits.

## 2. Add it as a custom OptiScaler

On the **Settings → Mods → OptiScaler** tab, click **Add Custom OptiScaler** and pick
the `.7z` in the file window. In the file preview, uncheck the readme/license files
(unchecked rows install nothing). The loader is staged as `dxgi.dll` automatically;
`OptiScaler.ini` and the bundled DLSS/Streamline DLLs ride along as companions.
Save the Add form.

## 3. Update `OptiScaler.ini`

On the new Mod card, expand **Files**, then click the **Edit** button beside
`OptiScaler.ini`.

For all Linux users, set this under `[NvApi]`:

```ini
DisableReflexSync = true
```

Ada Lovelace RTX 40xx users must also set these under their matching sections:

```ini
[NvApi]
DisableFlipMetering = true

[DLSSG]
AdaMfgUnlock = true
```

Save the file. These are the settings recommended by the fork's package `README.md`;
tune them if frame generation or motion pacing does not behave as expected.

## 4. Add `nvngx_dlssnr.dll` as a custom mod

The neural-rendering DLL ships in neither archive. On the **Settings → Mods → Custom Mods** tab,
click **Add Custom Mod…** and pick the DLL file itself in the file window. It keeps its filename — no slot
rename applies — and stages next to the loader, where the fork looks for it.

## 5. Install both for your game

On the Game → **Mods** tab, **Install** both for your game — the fork (e.g.
`OptiScaler - y4my4my4m - V4`) and `nvngx_dlssnr.dll` — then Play.
