use anyhow::Result;
use std::fs;
use std::path::Path;

use crate::ingest;

pub fn fetch_into(dir: &Path, chain: &str, address: &str) -> Result<ingest::FetchedSource> {
    fs::create_dir_all(dir.join("src"))?;
    let chain_id = ingest::chain_id_of(chain);
    let fetched = ingest::sourcify_files(chain_id, address)?;
    for (rel, body) in &fetched.files {
        let path = dir.join("src").join(rel);
        if let Some(p) = path.parent() {
            fs::create_dir_all(p)?;
        }
        fs::write(&path, body)?;
        println!("  wrote {}", path.display());
    }
    fs::write(dir.join("SURFACE.md"), render_surface(chain, address, &fetched))?;
    println!("surface {}", dir.join("SURFACE.md").display());
    println!("kind {}  heat {}  contract {}", fetched.kind, fetched.heat, fetched.contract_name);
    if !fetched.skip_reason.is_empty() {
        println!("skip: {}", fetched.skip_reason);
    }
    if !fetched.value_fns.is_empty() {
        println!("value fns: {}", fetched.value_fns.join(", "));
    }
    Ok(fetched)
}

pub fn render_surface(chain: &str, address: &str, f: &ingest::FetchedSource) -> String {
    let mut s = String::new();
    s.push_str("# surface\n\n");
    s.push_str(&format!("- chain: {chain}\n- ca: `{address}`\n"));
    s.push_str(&format!("- contract: {}\n- kind: {}\n- heat: {}\n", f.contract_name, f.kind, f.heat));
    s.push_str(&format!("- compiler: {}\n- deployer: `{}`\n", f.compiler, f.deployer));
    if !f.skip_reason.is_empty() {
        s.push_str(&format!("- skip: {}\n", f.skip_reason));
    }
    s.push_str("\n## value_fns\n");
    if f.value_fns.is_empty() { s.push_str("- (none matched)\n"); }
    else { for n in &f.value_fns { s.push_str(&format!("- `{n}`\n")); } }
    s.push_str("\n## abi_fns\n");
    if f.abi_fns.is_empty() { s.push_str("- (none — likely proxy fallback only)\n"); }
    else { for n in &f.abi_fns { s.push_str(&format!("- `{n}`\n")); } }
    s.push_str("\n## files\n");
    for (p, _) in &f.files { s.push_str(&format!("- `{p}`\n")); }
    s.push_str("\n## heat vs severity\nheat = skip|maybe|hunt (time).\nseverity = scout rate after a real hypothesis.\n");
    if f.kind == "proxy" {
        s.push_str("\nProxy shell. Read as proxy on the explorer and source the implementation.\n");
    }
    s
}

pub fn write_rate(dir: &Path, level: &str, note: &str) -> Result<()> {
    fs::write(dir.join("SEVERITY.md"), format!("severity: {level}\nnote: {note}\n"))?;
    let report = dir.join("REPORT.md");
    let mut r = if report.exists() { fs::read_to_string(&report)? } else { String::from("# draft\n") };
    if !r.contains("severity:") {
        r.push_str(&format!("\n- severity: {level}\n"));
        fs::write(report, r)?;
    }
    Ok(())
}
