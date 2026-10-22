# Game arguments

Per-game executable arguments. The rules are `docs/dev/app/plugin/game-args.md`.

`curated/` is hand-edited. `protonfixes/` is the backup annotation set written by `args-harvest`. Do not hand-edit harvested files. `pcgw/` and `lutris/` are not part of this tree.

Check the tree before committing catalog files:

```
cargo run -p tuxgt-core --bin args-check -- args
```

A row in this tree is not enabled for a game until someone sets it.
