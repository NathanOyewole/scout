# learn the hunt, not the course catalog

Scout finds CAs. You find bugs.

## week 1 — one contract

1. `scout pull --source sourcify --chain base`
2. `scout source <id>`
3. Read SURFACE.md. Those names move value.
4. Answer in NOTES.md: who can call this, what leaves, can a stranger hit a team path.
5. `scout test <id>` and fill testHypothesis.

If you cannot explain one function, do not hunt a second CA.

## study

- onlyOwner / roles / initialize on a proxy
- upgradeTo / delegatecall
- ERC20 transfer / transferFrom / allowance
- vault deposit withdraw harvest
- Foundry: vm.prank, vm.deal, vm.warp, fork test

Skip CEH, WiFi, Metasploit playlists.

## two queues

```
scout pull --address 0x... --chain base --kind bounty --program immunefi --name Protocol
scout pull --source sourcify --chain base --kind indie
scout list --kind bounty
scout list --kind indie
```

Listed program first if you need cash. Indie after.

Fork only. Private report. No mainnet call. No public drop while open.
