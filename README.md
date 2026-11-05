# scout v0.8

Heat on the radar. Severity on the report.

```bash
git pull
cargo install --path .
scout --version

scout pull --source immunefi --limit 5
scout list
scout surface 111

scout prove 111
scout rate 111 high --note "anyone can withdraw the vault balance"
scout report 111
scout rm 111

# Web dashboard (accessible from any device)
scout serve --port 8080
```

## commands

| Command | Description |
|---------|-------------|
| `scout pull` | Pull bounty targets from a radar source |
| `scout list` | List targets |
| `scout surface <id>` | Fetch verified source |
| `scout prove <id>` | Fork test |
| `scout rate <id> <level>` | Set severity rating |
| `scout report <id>` | Print (and finalize) the report |
| `scout hypothesis <id> <text>` | Store a hypothesis |
| `scout state <id> <state>` | Set target state |
| `scout rm <id>` | Drop a target |
| `scout dashboard` | Generate local triage dashboard HTML |
| `scout serve --port 8080` | Serve dashboard over HTTP (any device) |

## web dashboard

`scout serve --port 8080` starts an HTTP server. Access from any device on your network at `http://<your-ip>:8080`.

For public access from anywhere, run behind a tunnel:
```bash
scout serve --port 8080 &
cloudflared tunnel --url http://localhost:8080
```

## fork only, never live

This tool does not test against live contracts. No broadcast, no private keys. Fork test only.

State lives in `~/.scout`.

## Hermes plugin

If you use Hermes, the [Contract Hunter](https://github.com/NathanOyewole/hermes-contract-hunter) plugin wraps these commands as native tools, a `/hunt` slash command, and an optional session-start hook. Catalog PR: https://github.com/NousResearch/hermes-agent/pull/149478.
