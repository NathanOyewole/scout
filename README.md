# scout

Private hunt engine.

Find fresh contract addresses. Save them. Read them. Prove bugs on a local fork. Report privately.

```
scout pull     → Dexscreener latest CAs land in sqlite + cas.csv
scout list     → ranked queue
scout open 12  → NOTES.md + Foundry stub for that CA
```

No API key for v0. Live pull uses the public Dexscreener API.

## Install

```bash
git clone git@github.com:NathanOyewole/scout.git
cd scout
cargo install --path .
scout init
scout pull
scout list
```

Needs Rust (`rustup`) and a network for `scout pull`. Foundry is only needed when you write a PoC.

## Daily

```bash
scout pull                          # latest token profiles (new CAs)
scout pull --query "base vault"     # search
scout pull --query solana
scout pull --chain base --address 0xYourCA --name VaultX
scout list
scout open 4
scout state 4 hypothesized
scout state 4 proven                # only after the test prints the drain
scout report 4                      # private draft — do not post
scout export                        # rewrite ~/.scout/cas.csv
```

Offline:

```bash
scout pull --demo
```

## Where data lives

All local. Nothing is uploaded.

| path | what |
|---|---|
| `~/.scout/scout.db` | sqlite bus (CAs, score, state) |
| `~/.scout/cas.csv` | same CAs as a spreadsheet |
| `~/.scout/hunts/<id>-<name>/NOTES.md` | the 3 review questions |
| `~/.scout/hunts/<id>-<name>/REPORT.md` | private report draft |
| `~/.scout/hunts/<id>-<name>/poc/` | Foundry stub |

## How CAs are found

`scout pull` (no flags) hits:

`GET https://api.dexscreener.com/token-profiles/latest/v1`

Each row is a chain + token address + Dexscreener URL. That token address **is the CA**.

`scout pull --query base` hits Dexscreener search and stores the base token CA + pair.

That is not every unverified contract on a new L2. It is the fastest public feed of new tokens people are already pairing. Good enough to start hunting today. Next ingest sources: Sourcify recent verifies, Blockscout verified-contracts, a deployer watchlist.

## Review loop

1. Pick an id from `scout list`.
2. `scout open <id>` — open NOTES.md.
3. Pull **verified source** from the explorer (the live contract, not GitHub).
4. Answer:
   - Who is allowed to move the money?
   - What happens at the handoff between contracts?
   - Can a stranger trigger something only the team should?
5. Write one Foundry test against a **fork**. Never mainnet.
6. If the test shows forbidden value movement → `scout report <id>` and send privately.

## Rules

- Test on copies only.
- Do not inflate severity.
- Do not post details while the bug is open.

## Repo

Private: `github.com/NathanOyewole/scout`

```bash
git pull
cargo install --path .
```
