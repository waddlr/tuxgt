# RenoDX DLSS

RenoDX DLSS adds NVIDIA DLSS support to games through ReShade. You
need an NVIDIA GPU, and you supply the addon file yourself — TuxGT
sets it up but does not download it.

## 1. Hand TuxGT the addon

Open **Settings → Mods → ReShade** and find the **RenoDX DLSS** card.
Click **Provide files** and pick your copy from the
[RenoDX DLSS Discord](https://discord.gg/evYMwn5Vm) — the addon file itself,
or an archive or folder containing it.

## 2. Add the NVIDIA pieces

RenoDX DLSS also needs NVIDIA's Streamline files and a small proxy
file, both of which you supply. On **Settings → Mods → Custom Mods**:

- On the **NVIDIA Streamline** card, click **Provide files** and pick
  your Streamline files from [RenoDX DLSS Discord](https://discord.gg/evYMwn5Vm).
  You only need **nvngx_dlss.dll** the rest can be unchecked if it doesn't work for the game you are trying with.
- On the **nvngx_dlssnr - workaround proxy** card, click
  **Provide files** and pick a folder or archive containing two files: `nvngx_dlssnr.dll` proxy from this repo's releases, and the real NVIDIA file from the [RenoDX DLSS Discord](https://discord.gg/evYMwn5Vm), renamed to `nvngx_dlssnr.real.dll` so the proxy can find it. If you are not on 50xx series then make sure to get the one which works with your generation of GPU.

## 3. Turn it on for your game

On the Game → **Mods** tab, **Install** RenoDX DLSS. TuxGT asks to
install the other pieces it needs (ReShade, the `d3dcompiler_47`
helper, and the files above) — click **Install required**, then Play.

To check it worked, open the ReShade overlay in-game (Home key) and
confirm the addon is listed and enabled.
