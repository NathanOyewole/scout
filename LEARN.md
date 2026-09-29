# learn the hunt, not the course catalog

Scout finds CAs. You find bugs.

## week 1 — one contract

1. `scout pull --source sourcify --chain base`
2. `scout list` — triage on heat, not on score
3. `scout source <id>`
4. Read SURFACE.md. `admin_fns` next to `value_fns` is where the money is.
5. Answer in NOTES.md: who can call this, what leaves, can a stranger hit a team path.
6. `scout test <id>`, fill `testHypothesis` from NOTES.md, then `scout prove <id>`

`scout prove` will not mark anything proven while the test is still the empty
scaffold. That is the whole point — do not work around it.

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

## discipline

- A `skip` from a proxy shell means hunt the implementation, not the proxy.
- A sourcify `match` (not `exact_match`) means confirm the source is the
  deployed bytecode before you read a line of it.
- severity comes from a fork test, not from how bad the code looks.
- A green `forge test` on an empty `testHypothesis` proves nothing. `scout
  prove` checks for the stub first and refuses to mark anything proven.
- Prefer a stub that fails over a mock that passes. If a guard is not pinned
  by a test that fails before the fix, it is not a guard yet.

Fork only. Private report. No mainnet call. No public drop while open.
