# scout v0.4

One queue for listed bounties and independent hunts.

```bash
git pull
cargo install --path .
scout init

# indie / fresh verified
scout pull --source sourcify --chain base --kind indie
scout list --kind indie
scout source 1
scout test 1

# listed program CA you copied from Immunefi/Cantina
scout pull --address 0x... --chain eth --kind bounty --program immunefi --name ProtocolX
scout list --kind bounty
```

`scout test` prints a Foundry fork command. Never `--broadcast`.

See LEARN.md and ROADMAP.md.
