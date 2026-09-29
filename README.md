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
- `scout prove` refuses to run at all if a signing key is in the environment
  (`ETH_PRIVATE_KEY`, `PRIVATE_KEY`, `SOLANA_PRIVATE_KEY`, `MNEMONIC`, ...).
  There is no `--broadcast` anywhere in this crate and none may be added.

## fork only, never live

This tool does not test against live contracts, by design, not by
configuration:

- the only forge argv scout ever runs is
  `forge test --fork-url <url> -vv`, pinned by a test
- there is no `--broadcast`, no `--private-key`, no `forge script`, no
  `vm.startBroadcast`
- a private key in the environment is a hard error, not a warning

If you ever need a broadcast-capable tool, that is a different tool with
different rules. Do not bolt it onto this one.

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
- `ETH_PRIVATE_KEY` / `PRIVATE_KEY` / `SOLANA_PRIVATE_KEY` / `MNEMONIC` /
  `DEPLOYER_PRIVATE_KEY` / `FOUNDRY_PRIVATE_KEY` — `prove` refuses to run
  while any of these is set. Unset with `Remove-Item Env:NAME`.

## foundry

`scout prove` needs `forge`. Installed on this machine at
`C:\Users\Olufemi\.foundry\bin\forge.exe` (1.8.3), which is already on Path.

If you need to reinstall it, `foundryup` does not run in PowerShell, so grab
`foundry_v1.8.3_win32_amd64.zip` from
https://github.com/foundry-rs/foundry/releases, verify it against the
`.sha256` next to it, extract `forge.exe` into `~/.foundry/bin`, and put that
folder on Path. Without forge, `prove` says so and exits 127.

State lives in `~/.scout`, not in the repo: `scout.db`, `cas.csv`, and
`hunts/<id>-<name>/`.
