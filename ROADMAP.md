# scout roadmap

```
L1 radar  →  L2 surface  →  L3 hypothesis  →  L4 prove  →  L5 report
```

## Now — v0 radar
Shipped: sqlite bus, Dexscreener, Sourcify list, cas.csv, NOTES + Foundry stub.

## Next — v0.3 surface (this slice)
- [x] `scout source <id>` fetches Sourcify files into the hunt folder
- [x] list value-moving fn names from ABI
- [ ] persist compiler/proxy in sqlite
- [ ] github vs onchain diff

Done when: `scout source 12` drops `.sol` files on disk.

## Then
v0.4 hypothesis in db → v0.5 `scout test` on a fork → v0.6 private report → v1 extra radars (Blockscout, watchlist, Solana).

Cadence: `git pull && cargo install --path . && scout pull --source sourcify --chain base && scout source <id>`
