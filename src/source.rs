use anyhow::{Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::ingest;

pub fn fetch_into(dir: &Path, chain: &str, address: &str) -> Result<ingest::FetchedSource> {
    let chain_id = ingest::chain_id(chain)
        .with_context(|| format!("'{chain}' is not an EVM chain — scout pulls sources for EVM only (solana/near/btc are out of scope)"))?;
    let fetched = ingest::sourcify_files(chain_id, address)?;

    let src = dir.join("src");
    fs::create_dir_all(&src)?;
    let mut written = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    for (rel, body) in &fetched.files {
        // The path comes from a remote API. Joining it unchecked is a write
        // outside the hunt folder (../.. and C:\ and NUL all work).
        let Some(safe) = safe_rel_path(rel) else {
            skipped.push(rel.clone());
            continue;
        };
        let path = src.join(&safe);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
        written += 1;
    }
    if written == 0 {
        anyhow::bail!("every source path was rejected as unsafe — refusing to continue");
    }
    for s in &skipped {
        eprintln!("  skipped unsafe path from api: {s}");
    }
    println!("wrote {written} file(s) to {}", src.display());

    fs::write(
        dir.join("SURFACE.md"),
        render_surface(chain, address, &fetched),
    )?;
    if let Some(poc) = write_target_iface(dir, &fetched) {
        println!("iface  {}", poc.display());
    }
    println!("surface {}", dir.join("SURFACE.md").display());
    println!(
        "kind {}  heat {}  contract {}",
        fetched.kind, fetched.heat, fetched.contract_name
    );
    if !fetched.skip_reason.is_empty() {
        println!("skip: {}", fetched.skip_reason);
    }
    for n in &fetched.notes {
        println!("note: {n}");
    }
    if !fetched.value_fns.is_empty() {
        println!("value fns: {}", fetched.value_fns.join(", "));
    }
    if !fetched.admin_fns.is_empty() {
        println!("admin fns: {}", fetched.admin_fns.join(", "));
    }
    Ok(fetched)
}

/// Turn a remote source path into a relative path that is guaranteed to stay
/// inside the hunt directory. Returns None if anything is suspicious.
pub fn safe_rel_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim().replace('\\', "/");
    if trimmed.is_empty() {
        return None;
    }
    // Reject NUL and any component that could escape or land on a device.
    if trimmed.contains('\0') || trimmed.contains(':') {
        return None;
    }
    let p = Path::new(&trimmed);
    if p.is_absolute() {
        return None;
    }

    let mut parts: Vec<String> = Vec::new();
    for c in p.components() {
        match c {
            Component::Normal(os) => {
                let s = os.to_str()?;
                if s.is_empty() || s == "." || s == ".." {
                    return None;
                }
                parts.push(s.to_string());
            }
            Component::CurDir => {}
            // ParentDir, RootDir, Prefix: never allowed in a remote path.
            _ => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    // Windows refuses these, and npm-style trees do contain them.
    let cleaned: Vec<String> = parts
        .iter()
        .map(|s| {
            let upper = s.to_ascii_uppercase();
            let stem = upper.split('.').next().unwrap_or("");
            const RESERVED: [&str; 22] = [
                "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
                "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8",
                "LPT9",
            ];
            if RESERVED.contains(&stem) {
                format!("_{s}")
            } else {
                s.clone()
            }
        })
        .collect();
    // Longest path any modern toolchain accepts.
    if cleaned.join("/").len() > 200 {
        return None;
    }
    Some(cleaned.join("/"))
}

pub fn render_surface(chain: &str, address: &str, f: &ingest::FetchedSource) -> String {
    let mut s = String::new();
    s.push_str("# surface\n\n");
    s.push_str(&format!("- chain: {chain}\n- ca: `{address}`\n"));
    s.push_str(&format!(
        "- contract: {}\n- kind: {}\n- heat: {}\n",
        f.contract_name, f.kind, f.heat
    ));
    s.push_str(&format!(
        "- compiler: {}\n- deployer: `{}`\n",
        f.compiler, f.deployer
    ));
    s.push_str(&format!(
        "- sourcify match: {}\n- files: {}\n",
        if f.match_kind.is_empty() {
            "?"
        } else {
            &f.match_kind
        },
        f.files.len()
    ));
    s.push_str(&format!(
        "- abi: {} function(s), {} state-changing\n",
        f.abi_fns.len(),
        f.value_fns.len() + f.admin_fns.len()
    ));
    if !f.skip_reason.is_empty() {
        s.push_str(&format!("- skip: {}\n", f.skip_reason));
    }
    for n in &f.notes {
        s.push_str(&format!("- note: {n}\n"));
    }
    s.push_str("\n## value_fns (state changing, moves money)\n");
    if f.value_fns.is_empty() {
        s.push_str("- (none matched)\n");
    } else {
        for n in &f.value_fns {
            s.push_str(&format!("- `{n}`\n"));
        }
    }
    s.push_str("\n## admin_fns (state changing, moves control)\n");
    if f.admin_fns.is_empty() {
        s.push_str("- (none matched)\n");
    } else {
        for n in &f.admin_fns {
            s.push_str(&format!("- `{n}`\n"));
        }
    }
    s.push_str("\n## read_fns (view — reading these is free, calling them is not)\n");
    if f.read_fns.is_empty() {
        s.push_str("- (none matched)\n");
    } else {
        for n in f.read_fns.iter().take(40) {
            s.push_str(&format!("- `{n}`\n"));
        }
        if f.read_fns.len() > 40 {
            s.push_str(&format!("- … and {} more\n", f.read_fns.len() - 40));
        }
    }
    s.push_str("\n## files\n");
    for (p, _) in &f.files {
        s.push_str(&format!("- `{p}`\n"));
    }
    s.push_str("\n## heat vs severity\nheat = skip|maybe|hunt (how much time this is worth).\nseverity = `scout rate` after you have a hypothesis and a fork test.\n");
    if f.kind == "proxy" {
        s.push_str("\nProxy shell. Read `implementation()` on the fork, then `scout pull --address <impl>`.\n");
    }
    if f.kind == "upgradeable" {
        s.push_str("\nUUPS. The implementation can be swapped. `_authorizeUpgrade` is the whole ballgame.\n");
    }
    s
}

/// A typed interface for the state-changing surface, so a fork test can call
/// the target directly instead of abi.encodeWithSignature by hand.
pub fn write_target_iface(dir: &Path, f: &ingest::FetchedSource) -> Option<PathBuf> {
    if f.decls.is_empty() {
        return None;
    }
    let mut s = String::new();
    s.push_str("// SPDX-License-Identifier: MIT\n");
    s.push_str(&format!(
        "// generated by scout {} from the verified abi. do not edit.\n",
        env!("CARGO_PKG_VERSION")
    ));
    s.push_str("pragma solidity ^0.8.0;\n\n");
    s.push_str("interface ITarget {\n");
    for d in &f.decls {
        s.push_str(&format!("    {} // {}\n", d.decl, d.sig));
    }
    s.push_str("}\n");
    let poc_src = dir.join("poc/src");
    fs::create_dir_all(&poc_src).ok()?;
    let path = poc_src.join("TargetCalls.sol");
    fs::write(&path, s).ok()?;
    Some(path)
}

pub fn write_rate(dir: &Path, level: &str, note: &str) -> Result<()> {
    fs::create_dir_all(dir)?;
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    fs::write(
        dir.join("SEVERITY.md"),
        format!("severity: {level}\nnote: {note}\nrated: {stamp}\n"),
    )?;

    let report = dir.join("REPORT.md");
    let mut r = if report.exists() {
        fs::read_to_string(&report)?
    } else {
        String::from("# draft\n")
    };
    // The old code only appended when "severity:" was absent, so rating an
    // existing draft (whose template already had the key) silently did nothing.
    let line = format!("- severity: {level}");
    if r.lines().any(|l| l.trim_start().starts_with("- severity:")) {
        let mut out = Vec::new();
        for l in r.lines() {
            if l.trim_start().starts_with("- severity:") {
                out.push(line.clone());
            } else {
                out.push(l.to_string());
            }
        }
        r = out.join("\n");
        if !r.ends_with('\n') {
            r.push('\n');
        }
    } else {
        if !r.ends_with('\n') {
            r.push('\n');
        }
        r.push_str(&line);
        r.push('\n');
    }
    if !note.trim().is_empty() {
        let n = format!("- note: {}", note.trim());
        if !r.contains(&n) {
            r.push_str(&n);
            r.push('\n');
        }
    }
    fs::write(&report, r)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traversal_and_absolute_paths_are_rejected() {
        for bad in [
            "../../../windows/system32/drivers/etc/hosts",
            "..\\..\\evil.sol",
            "/etc/passwd",
            "C:/Windows/System32/evil.sol",
            "C:\\Windows\\evil.sol",
            "a/../../b.sol",
            "",
            "   ",
            "src/..",
            "./",
            "nested/../../escape.sol",
        ] {
            assert_eq!(safe_rel_path(bad), None, "{bad:?} must be rejected");
        }
    }

    #[test]
    fn nul_and_alternate_data_streams_are_rejected() {
        assert_eq!(safe_rel_path("a.sol\0.txt"), None);
        assert_eq!(safe_rel_path("evil.sol:hidden"), None);
    }

    #[test]
    fn normal_source_paths_survive() {
        assert_eq!(
            safe_rel_path("src/AgentWalletV2.sol").as_deref(),
            Some("src/AgentWalletV2.sol")
        );
        assert_eq!(
            safe_rel_path("lib/openzeppelin-contracts/contracts/token/ERC20/IERC20.sol").as_deref(),
            Some("lib/openzeppelin-contracts/contracts/token/ERC20/IERC20.sol")
        );
        assert_eq!(
            safe_rel_path("contracts\\Foo.sol").as_deref(),
            Some("contracts/Foo.sol")
        );
        assert_eq!(
            safe_rel_path("contracts/@openzeppelin/Access.sol").as_deref(),
            Some("contracts/@openzeppelin/Access.sol")
        );
    }

    #[test]
    fn windows_device_names_are_escaped_not_written_verbatim() {
        assert_eq!(
            safe_rel_path("con/Aux.sol").as_deref(),
            Some("_con/_Aux.sol")
        );
        assert_eq!(safe_rel_path("LPT1.txt").as_deref(), Some("_LPT1.txt"));
    }

    #[test]
    fn overlong_paths_are_rejected_instead_of_truncated() {
        let deep = format!(
            "{}/x.sol",
            vec!["dir_that_is_quite_long_indeed"; 40].join("/")
        );
        assert_eq!(safe_rel_path(&deep), None);
    }

    #[test]
    fn rate_rewrites_an_existing_severity_line() {
        let d = std::env::temp_dir().join("scout_rate_test");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("REPORT.md"), "# draft\n- severity:\n- impact:\n").unwrap();
        write_rate(&d, "high", "anyone can drain").unwrap();
        let r = fs::read_to_string(d.join("REPORT.md")).unwrap();
        assert!(r.contains("- severity: high"), "{r}");
        assert!(
            !r.contains("- severity:\n"),
            "blank severity must be gone: {r}"
        );
        assert!(r.contains("- impact:"), "template must survive: {r}");
        // Re-rating must replace, not stack.
        write_rate(&d, "critical", "").unwrap();
        let r2 = fs::read_to_string(d.join("REPORT.md")).unwrap();
        assert_eq!(r2.matches("- severity:").count(), 1, "{r2}");
        assert!(r2.contains("- severity: critical"), "{r2}");
        let _ = fs::remove_dir_all(&d);
    }
}
