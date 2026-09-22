# Deep Fried Chicken v3

Deep Fried Chicken (DFC) adds NVIDIA DLSS frame generation to games
through ReShade. It works on 64-bit games with an NVIDIA GPU. You get
the DFC files yourself — TuxGT sets them up but does not download them.

## 1. Hand TuxGT the DFC files

Open **Settings → Mods → ReShade** and find the
**Deep Fried Chicken v3 - 64bit** card. Click **Provide files** and pick
your [Deep Fried Chicken Discord](https://discord.gg/dUuY6ZfvM) — the `.7z` archive or the `64-bit`
folder from the package. If the archive asks for a password, TuxGT prompts you for it.
The Missing list on the card empties once it has all four files.

## 2. Add the NVIDIA pieces

DFC also needs NVIDIA's Streamline files and a small proxy file, both
of which you supply. On **Settings → Mods → Custom Mods**:

- On the **NVIDIA Streamline** card, click **Provide files** and pick
  your Streamline files from [Deep Fried Chicken Discord](https://discord.gg/dUuY6ZfvM).
  You only need **nvngx_dlss.dll** the rest can be unchecked if it doesn't work for the game you are trying with.
- On the **nvngx_dlssnr - workaround proxy** card, click
  **Provide files** and pick a folder or archive containing two files: `nvngx_dlssnr.dll` proxy from this repo's releases, and the real NVIDIA file from the [RenoDX DLSS Discord](https://discord.gg/evYMwn5Vm), renamed to `nvngx_dlssnr.real.dll` so the proxy can find it. If you are not on 50xx series then make sure to get the one which works with your generation of GPU.

## 3. Turn it on for your game

On the Game → **Mods** tab, **Install** Deep Fried Chicken. TuxGT asks
to install the other pieces it needs (ReShade, the `d3dcompiler_47`
helper, and the files above) — click **Install required**, then Play.

To check it worked, open the ReShade overlay in-game (Home key) and
confirm the addon is listed and enabled.
