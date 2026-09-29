# scout v0.7

Heat on the radar. Severity on the report.

```powershell
git pull
cargo install --path .
scout --version

scout pull --source sourcify --chain base --kind indie
scout list
scout source 4
# kind / heat / value_fns / admin_fns, heat=skip auto-marks killed
# writes SURFACE.md, the source tree, and poc/src/TargetCalls.sol

scout prove 4      # fork test. never marks proven off the empty scaffold
scout rate 4 high --note "anyone can withdraw the vault balance"
scout report 4
scout rm 4         # drop a false positive (refuses if it is graded)
```

## what each command guarantees

- `scout source` only ever writes inside the hunt folder. Remote paths that
  try to escape it are rejected, not followed.
- `scout prove` refuses to set `proven` while `testHypothesis` is still the
  generated stub. A green run that proved nothing leaves the state alone.
- `scout report` refuses to set `reported` with no severity on the target.
- `scout rm` refuses to delete a target that has a severity, unless `--force`.
- `cas.csv` is rewritten after every state change, so it never disagrees with
  the database. This is the file you paste into a submission.
- A target's `name` is written once, on insert. Re-pulling the same address
  refreshes the score, tags and facts but never the name, because the hunt
  folder is named after it.
- `scout prove` exits `127` when forge is not installed, so a script can tell
  "tool missing" apart from "the test failed".

## reading SURFACE.md

- `value_fns` — state-changing, looks like it moves money.
- `admin_fns` — every other state-changing entry. The knobs.
- `read_fns` — view/pure. Reading is free, calling is a transaction.
- `heat` — skip | maybe | hunt. How much time it is worth. A control plane
  next to a value plane is the hot shape. A sourcify `match` instead of
  `exact_match` downgrades a hunt to a maybe: the source may not be the
  bytecode on chain.
- `kind` — `proxy` means hunt the implementation, not this address.
  `upgradeable` means read `_authorizeUpgrade`.

`scout rate <id> info|low|medium|high|critical` writes SEVERITY.md and updates
the severity line in REPORT.md. You grade the hypothesis, not the CA.

## environment

- `SCOUT_RPC` — override the fork url. Public endpoints rate limit.

## foundry

`scout prove` needs `forge`. `foundryup` does not run in PowerShell, so grab
the win32 zip from https://github.com/foundry-rs/foundry/releases, extract
`forge.exe`, and put that folder on Path. Without it `prove` prints exactly
that and exits 127.
