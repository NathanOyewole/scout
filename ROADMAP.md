# scout roadmap

Build order is the engine, not features.

```
L1 radar  →  L2 surface  →  L3 hypothesis  →  L4 prove  →  L5 report
```

A slice is done when you use it on a real CA the same day. Not when it looks complete.

---

## Now — v0 radar (in progress)

**Goal:** `scout pull` fills a local queue with real CAs. You can open one and start reading.

| slice | status |
|---|---|
| sqlite bus + `init/list/open/report/state` | shipped |
| Dexscreener latest profiles + search | shipped |
| `cas.csv` export | shipped |
| workspace NOTES + Foundry stub | shipped |
| Sourcify recent verified contracts | **this push** |
| README you can follow without asking | shipped |

Done when: you run `scout pull --source sourcify --chain base` and get live verified CAs.

---

## Next — v0.3 surface

Pull the actual source, not just the address.

- [ ] `scout source <id>` — fetch Sourcify files for that CA into the hunt folder
- [ ] save compiler, proxy flag, file list in sqlite
- [ ] list function names that look like they move value (`withdraw`, `harvest`, `mint`, `execute`, `upgradeTo`)
- [ ] mark `github != onchain` if both exist later

Done when: `scout open 12` also drops `.sol` files on disk.

---

## v0.4 hypothesis

- [ ] force the 3 Duke questions in NOTES (already templated)
- [ ] `scout note <id> --h1 "..."` so hypotheses live in the db
- [ ] optional AI plugin: surface in → max 3 testable claims out
- [ ] reject a claim that has no test hook

Done when: a target cannot sit in `scoped` without at least one H1.

---

## v0.5 prove

- [ ] one shared Foundry repo instead of a stub per hunt
- [ ] `scout test <id>` runs `forge test --fork-url $RPC`
- [ ] state flips to `proven` or `killed` from the test result
- [ ] Solana path: LiteSVM stub (later than EVM)

Done when: you have one killed hypothesis and one proven one in the db.

---

## v0.6 report

- [ ] `scout report <id>` always dumps the same short template
- [ ] attach the passing test
- [ ] never auto-post

---

## v1 radar (after the loop works)

Only add sources that change what you hunt this week.

- [ ] Blockscout `verified-contracts` for new L2s (Arc and friends)
- [ ] deployer watchlist file (`~/.scout/watch.txt`)
- [ ] 0-follower / new-X account catcher
- [ ] Solana recent program deploys

Not v1: NFT sniping, call groups, “AI auditor,” Web2 subdomain recon as the core.

---

## Cadence

You: `git pull && cargo install --path . && scout pull`

This side: one slice per session. Push to `NathanOyewole/scout` on `main`.

If a slice is not used within 14 days, cut it.
