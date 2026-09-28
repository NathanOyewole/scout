# scout

Private hunt engine.

Find fresh contract addresses. Save them. Read them. Prove bugs on a local fork. Report privately.

```
scout pull     → CAs land in sqlite + cas.csv
scout list     → ranked queue
scout open 12  → NOTES.md + Foundry stub
```

No API key. Dexscreener + Sourcify are public.

Build order: [ROADMAP.md](./ROADMAP.md)

## Install

```bash
git clone git@github.com:NathanOyewole/scout.git
cd scout
cargo install --path .
scout init
scout pull
scout list
```

Needs Rust. Foundry is only needed when you write a PoC.

## Daily

```bash
git pull && cargo install --path .

scout pull                                    # Dexscreener latest CAs
scout pull --query "base vault"               # Dexscreener search
scout pull --source sourcify --chain base     # latest verified on Base
scout pull --source sourcify --chain eth
scout pull --source all --chain base          # both feeds
scout pull --chain base --address 0xYourCA --name VaultX

scout list
scout open 4
scout state 4 hypothesized
scout state 4 proven
scout report 4
scout export
```

Offline: `scout pull --demo`

## Where data lives

All local. Nothing is uploaded.

| path | what |
|---|---|
| `~/.scout/scout.db` | sqlite bus |
| `~/.scout/cas.csv` | same CAs as a file |
| `~/.scout/hunts/<id>-<name>/NOTES.md` | review questions |
| `~/.scout/hunts/<id>-<name>/REPORT.md` | private draft |
| `~/.scout/hunts/<id>-<name>/poc/` | Foundry stub |

## How CAs are found

- `scout pull` → Dexscreener token profiles. Address = CA.
- `scout pull --query base` → Dexscreener search. Base token + pair.
- `scout pull --source sourcify --chain base` → Sourcify `/v2/contracts/8453`. Already verified. Better hunt targets.

Next source after you use these two: Blockscout on new L2s.

## Review loop

1. `scout list` → pick an id
2. `scout open <id>`
3. Pull verified source from the explorer
4. Answer the three questions in NOTES.md
5. Prove on a fork. Never mainnet.
6. `scout report <id>` — send privately

## Rules

- Test on copies only
- Do not inflate severity
- Do not post details while the bug is open
