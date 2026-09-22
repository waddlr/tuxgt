# OptiScaler y4my4m fork

A third-party OptiScaler build with DLSS frame generation: the
[y4my4my4m DLSS-NR multipass MFG fork](https://github.com/y4my4my4m/OptiScaler_DLSSNR_Multipass_MFG).
TuxGT installs the v4 release with its DLSS files included; you only
add one NVIDIA file yourself. OptiScaler works on 64-bit games only.

## 1. Install the fork for your game

On the Game → **Mods** tab, **Install** **OptiScaler - y4my4m - v4**.
TuxGT downloads it on first install.

On the installed card, set **Slot** to `dxgi` (or another free proxy: `d3d11`, `d3d12`, `winmm`, `version`). Preload keeps `OptiScaler.dll`
until you change it. If this game's Adapter is **Install**, TuxGT
already asks and defaults to `dxgi`.

Do not also enable ReShade for this game. Stock-named ReShade plus
OptiScaler on the `dxgi` preload slot leaves ReShade unloaded.

## 2. Fix the settings file

On the installed card, expand **Files**, then click the **Edit**
button beside `OptiScaler.ini`. (This copy is per game.)

Under `[NvApi]`, set:

```ini
DisableReflexSync = true
```

On RTX 40xx cards, also set under the matching sections:

```ini
[NvApi]
DisableFlipMetering = true

[DLSSG]
AdaMfgUnlock = true
```

For DLSS neural rendering, set under `[DlssNr]`:

```ini
Enabled = true
```

`DualFeature = true` is a separate layout: the first upscaler half
writes at render resolution, the model runs there, and `DualEnlarger`
is the stretch to display (`auto` is a spatial scaler; or `dlss`,
`fsr22`, `fsr31`, `ffx`, `xess`). Off, NR runs on the display-sized
output. That layout does nothing at DLAA. A change needs a restart or
a quality switch.

Save the file. These come from the fork's own docs; adjust the FG keys
if frame generation stutters or the pacing feels off. Neural rendering
stays off until `Enabled` is set.

## 3. Add the one NVIDIA file

One file ships in neither download: NVIDIA's neural-rendering library.
On **Settings → Mods → Custom Mods**, click **Provide files** on
**nvngx_dlssnr** and pick your copy from the [RenoDX DLSS Discord](https://discord.gg/evYMwn5Vm).
If you are not on 50xx series then make sure to get the one which works with your generation of GPU.
Then install it for the game on the Game → **Mods** tab.

If that doesn't work on your system, use the
**nvngx_dlssnr - workaround proxy** instead, click
**Provide files** and pick a folder or archive containing two files: `nvngx_dlssnr.dll` proxy from this repo's releases, and the real NVIDIA file from the [RenoDX DLSS Discord](https://discord.gg/evYMwn5Vm), renamed to `nvngx_dlssnr.real.dll` so the proxy can find it. If you are not on 50xx series then make sure to get the one which works with your generation of GPU.

## 4. Play

In-game, open the OptiScaler overlay with Insert to confirm it loaded.
