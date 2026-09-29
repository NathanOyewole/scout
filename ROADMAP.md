# scout roadmap

```
radar -> surface -> hypothesis -> prove -> report
```

- [x] v0 radar
- [x] v0.3 source
- [x] v0.4 queues + test print
- [x] v0.5 heat + rate
- [x] v0.6 prove (runs forge fork test if forge is installed)
- [x] v0.7 correctness: it actually compiles, and the guarantees hold
  - value vs admin vs read surface split by state mutability
  - proxy shells skip even when they expose `upgradeTo`
  - partial sourcify match downgrades a hunt
  - non-EVM finds are dropped instead of looked up on base
  - remote source paths cannot escape the hunt folder
  - a stub `testHypothesis` can no longer mark a target proven
  - `report` needs a severity, `rm` needs `--force` on a graded target
  - `cas.csv` never goes stale
  - generated `poc/src/TargetCalls.sol` so a fork test can call the target
  - http timeout + user-agent + 429/5xx retry with Retry-After
  - `SCOUT_RPC` fork override
  - 43 unit tests, clippy clean
 - target `name` is write-once, so a re-pull can never move a hunt folder
 - generated interface keeps overloaded solidity signatures
 - `prove` exits 127 when forge is missing, and every mutating command
   refreshes `cas.csv` on drop
- [ ] hypothesis rows in sqlite
- [ ] v1 extra radars / Immunefi ingest

## known gaps

- `forge` is not installed on this machine, so `scout prove` exits 127. The
  scaffold, the interface codegen and the placeholder guard are all in place;
  only the binary is missing.
- heat is a keyword heuristic over the abi. It is triage, not a verdict. The
  Duke questions in NOTES.md are still the real work.
