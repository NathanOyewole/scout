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
  - it compiled again: two borrow-after-move errors had made
    `cargo install --path .` fail since v0.5, so the binary on Path was
    still v0.4.0
  - value vs admin vs read surface split by state mutability
  - proxy shells skip even when they expose `upgradeTo`
  - partial sourcify match downgrades a hunt
  - non-EVM finds are dropped instead of looked up on base
  - remote source paths cannot escape the hunt folder
  - a stub `testHypothesis` can no longer mark a target proven
  - `report` needs a severity, `rm` needs `--force` on a graded target
  - `cas.csv` never goes stale (refreshed on `Hunt::drop`)
  - generated `poc/src/TargetCalls.sol` so a fork test can call the target
  - reference types carry a data location; signatures never do, because the
    sig feeds the selector
  - addresses are EIP-55 checksummed, because solc rejects a mis-cased
    literal and addresses are stored lowercase
  - generated solidity stays ASCII: solc rejects a non-ASCII char in a
    string literal
  - target `name` is write-once, so a re-pull can never move a hunt folder
  - generated interface keeps overloaded solidity signatures
  - `prove` exits 127 when forge is missing
  - http timeout + user-agent + 429/5xx retry with Retry-After
  - `SCOUT_RPC` fork override
  - 52 unit tests, clippy clean, `cargo fmt` clean
- [x] hypothesis rows in sqlite
- [x] v1 extra radars / Immunefi ingest

## fork only, never live

Hard rule, enforced in code and pinned by tests:

- the only forge argv scout runs is `forge test --fork-url <url> -vv`
- no `--broadcast`, no `--private-key`, no `forge script`, no
  `vm.startBroadcast`, anywhere in the crate
- `prove` refuses to start if a signing key is in the environment

Do not add a broadcast path. If a report needs a live call, that is a
separate tool with separate rules.

## known gaps

- heat is a keyword heuristic over the abi. It is triage, not a verdict. The
  Duke questions in NOTES.md are still the real work.
- a sourcify `match` that is not `exact_match` downgrades the heat but cannot
  verify the source against deployed bytecode. Confirm it before reading a
  line of the code.
- public fork endpoints rate limit and occasionally time out. `SCOUT_RPC`
  exists for that.
