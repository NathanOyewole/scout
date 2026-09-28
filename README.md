# scout

```
scout pull --source sourcify --chain base
scout list
scout source <id>
```

Drops verified `.sol` files into `~/.scout/hunts/<id>/src/` plus `SURFACE.md`.

## Install

```bash
git clone git@github.com:NathanOyewole/scout.git
cd scout
git pull
cargo install --path .
scout init
scout pull --source sourcify --chain base
scout list
scout source 1
```

Needs Rust. No API key.

## Commands

| cmd | what |
|---|---|
| `scout pull` | Dexscreener CAs |
| `scout pull --source sourcify --chain base` | latest verified CAs |
| `scout pull --source all --chain base` | both feeds |
| `scout list` | queue |
| `scout open <id>` | NOTES + Foundry stub |
| `scout source <id>` | download verified source |
| `scout report <id>` | private draft |

Data: `~/.scout/scout.db`, `~/.scout/cas.csv`, `~/.scout/hunts/`.

See ROADMAP.md.
