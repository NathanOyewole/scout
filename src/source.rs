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
    let mut meta = format!(
        "compiler: {}\ndeployer: {}\nvalue_fns:\n",
        fetched.compiler, fetched.deployer
    );
    for f in &fetched.value_fns {
        meta.push_str(&format!("- {f}\n"));
    }
    fs::write(dir.join("SURFACE.md"), meta)?;
    println!("surface -> {}", dir.join("SURFACE.md").display());
    if !fetched.value_fns.is_empty() {
        println!("value fns: {}", fetched.value_fns.join(", "));
    }
    Ok(fetched)
}
