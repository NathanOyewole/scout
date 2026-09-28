# scout v0.5

Heat on the radar. Severity on the report.

```powershell
git pull
cargo install --path .
scout --version

scout pull --source sourcify --chain base --kind indie
scout source 4
# prints kind / heat / skip reason
# heat=skip auto-marks killed

scout rate 4 high
```

SURFACE.md now has contract name, kind (proxy|impl|token|vault), heat (skip|maybe|hunt), abi names, file list.

`scout rate <id> info|low|medium|high|critical` writes SEVERITY.md. You grade the hypothesis, not the CA.
