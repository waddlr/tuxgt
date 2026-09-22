# Prefix tools (General)

Wine/Proton **Tools** on Game → General. Not a tab. Not a Proton installer.

**Tools** — `winecfg`, `regedit`, `explorer`, winetricks. Wine builtins
run through the detected system wine (or a direct-path proton) with
`WINEPREFIX` (+ `STEAM_COMPAT_DATA_PATH`); `winetricks` from `PATH`. A missing
runner is an honest status line, never a disabled placeholder.

Native: muted “no Wine prefix”. Install/prefix paths are About buttons.

Do not paint: identity dump, prefix tree, doctor, snapshots, shader-cache.

Code: `src/tuxgt/tuxgt-app/src/gui/prefix.rs`.
