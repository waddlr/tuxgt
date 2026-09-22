# Game identity

Primary key: `manager_id:store_id:game_id`

- Empty `store_id` when the manager *is* the store (Steam owned apps).
- One `standalone` store for the standalone class: Steam non-Steam shortcuts, Heroic sideloads, manual rows.
- No UUIDs. Where the store does not already give a short id: **8 chars**, `A-Za-z0-9`, collision retry.

| Kind | Id |
|---|---|
| Steam app | `steam::814380` |
| Steam non-Steam shortcut | `steam:standalone:<appid>` |
| Heroic GOG | `heroic:gog:<gogid>` |
| Heroic Epic | `heroic:epic:<app_name>` |
| Heroic Amazon | `heroic:amazon:<id>` |
| Heroic sideload | `heroic:standalone:<heroic_app_name>` |
| Manual (TuxGT only) | `manual:standalone:<8id>` |

Optional overlay: a Steam AppID on any game (`games.steam_appid`) so Heroic/manual rows reach
ProtonDB + SteamGridDB. Resolution: stored overlay wins, else the Steam game segment when
`manager == "steam"`, else none. Scan never writes the column, so the overlay survives rescan
(a pruned row — gone from its store — loses it). Display name is never the key.

```
tuxgt games appid <id>              # print id<TAB>appid (empty when unset)
tuxgt games appid <id> <appid>      # store a 1-10 ASCII-digit appid (trimmed)
tuxgt games appid <id> --clear      # clear the overlay
tuxgt games appid-search <name>     # keyless Steam Store search, appid<TAB>name lines
```

Unknown id → `unknown game`; non-digit / empty-as-set → `invalid override`.

GUI: Game → General → About has a **Steam AppID** row — Steam rows show the
resolved id + Copy; Heroic/manual show input + Save + Search while unset and
the stored value + Clear once set. Search runs a keyless Steam Store name
lookup over the input text and lists candidates; picking one saves it like
Save. Same trim + 1-10 ASCII-digit validation; core errors surface in the
status line. Save with an empty input clears. Saving (or clearing) refreshes
metadata (`gui.metadata-fetch`) and hero art (SteamGridDB heroes still need a
keyring key).

Scan writers emit `standalone`; legacy `shortcut` / `sideload` / empty store ids were migrated.
