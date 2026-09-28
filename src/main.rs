use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

mod db;
mod ingest;

#[derive(Parser)]
#[command(name = "scout", about = "Find fresh CAs. Review offline. Report privately.", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create ~/.scout and the sqlite bus
    Init,
    /// Find CAs (live Dexscreener) or add one by hand
    Pull {
        /// Search query, e.g. "base vault" or a ticker
        #[arg(long)]
        query: Option<String>,
        /// How many results to keep
        #[arg(long, default_value_t = 25)]
        limit: usize,
        /// Offline fake targets (no network)
        #[arg(long)]
        demo: bool,
        /// Add a specific CA
        #[arg(long)]
        address: Option<String>,
        #[arg(long, default_value = "base")]
        chain: String,
        #[arg(long, default_value = "unnamed")]
        name: String,
        #[arg(long, default_value = "")]
        deployer: String,
    },
    /// Ranked queue
    List,
    /// Dump ~/.scout/cas.csv
    Export,
    /// Write workspace (notes + Foundry stub)
    Open { id: i64 },
    /// Print private report draft
    Report { id: i64 },
    /// seen|scored|scoped|hypothesized|proven|killed|reported|patched
    State { id: i64, state: String },
}

fn root() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".scout")
}
fn db_path() -> PathBuf {
    root().join("scout.db")
}
fn hunts_dir() -> PathBuf {
    root().join("hunts")
}
fn csv_path() -> PathBuf {
    root().join("cas.csv")
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init => cmd_init(),
        Cmd::Pull {
            query,
            limit,
            demo,
            address,
            chain,
            name,
            deployer,
        } => cmd_pull(query, limit, demo, address, chain, name, deployer),
        Cmd::List => cmd_list(),
        Cmd::Export => cmd_export(),
        Cmd::Open { id } => cmd_open(id),
        Cmd::Report { id } => cmd_report(id),
        Cmd::State { id, state } => cmd_state(id, state),
    }
}

fn cmd_init() -> Result<()> {
    fs::create_dir_all(hunts_dir())?;
    let _ = db::open(&db_path())?;
    println!("scout ready");
    println!("  db    {}", db_path().display());
    println!("  csv   {}", csv_path().display());
    println!("  hunts {}", hunts_dir().display());
    println!("\nnext: scout pull");
    Ok(())
}

fn cmd_pull(
    query: Option<String>,
    limit: usize,
    demo: bool,
    address: Option<String>,
    chain: String,
    name: String,
    deployer: String,
) -> Result<()> {
    fs::create_dir_all(hunts_dir())?;
    let conn = db::open(&db_path())?;

    if let Some(addr) = address {
        let id = db::upsert(
            &conn,
            &chain,
            &addr,
            &name,
            &deployer,
            "",
            "",
            "manual",
            4,
            &["manual".into()],
        )?;
        println!("#{id}  {chain}  {addr}");
        let n = db::export_csv(&conn, &csv_path())?;
        println!("saved {n} CAs → {}", csv_path().display());
        return Ok(());
    }

    if demo {
        for (chain, addr, name, sigs) in [
            (
                "base",
                "0x1111111111111111111111111111111111111111",
                "FreshVault",
                vec!["demo", "vault"],
            ),
            (
                "solana",
                "Scout1111111111111111111111111111111111111",
                "Launchpad",
                vec!["demo", "launchpad"],
            ),
        ] {
            let extra: Vec<String> = sigs.into_iter().map(|s| s.to_string()).collect();
            db::upsert(&conn, chain, addr, name, "", "", "", "demo", 3, &extra)?;
        }
        let n = db::export_csv(&conn, &csv_path())?;
        println!("demo queue. saved {n} CAs → {}", csv_path().display());
        return Ok(());
    }

    let found = if let Some(q) = query {
        println!("searching dexscreener for `{q}` …");
        ingest::search(&q, limit)?
    } else {
        println!("pulling latest dexscreener token profiles …");
        ingest::latest_profiles(limit)?
    };

    if found.is_empty() {
        println!("nothing returned. try: scout pull --query base");
        return Ok(());
    }

    for f in &found {
        let id = db::upsert(
            &conn,
            &f.chain,
            &f.address,
            &f.name,
            "",
            &f.pair,
            &f.url,
            &f.source,
            f.score,
            &f.signals,
        )?;
        println!(
            "#{:<4} {:<10} {:<16} {}  {}",
            id,
            f.chain,
            trunc(&f.name, 16),
            f.address,
            f.url
        );
    }
    let n = db::export_csv(&conn, &csv_path())?;
    println!("\n{n} CAs in db + {}", csv_path().display());
    println!("open one: scout open <id>");
    Ok(())
}

fn cmd_list() -> Result<()> {
    let conn = db::open(&db_path())?;
    let rows = db::list(&conn)?;
    if rows.is_empty() {
        println!("empty. run: scout pull");
        return Ok(());
    }
    println!(
        "{:>4}  {:<10} {:<8} {:>5}  {:<16} {}",
        "id", "chain", "state", "score", "name", "address"
    );
    for t in rows {
        println!(
            "{:>4}  {:<10} {:<8} {:>5}  {:<16} {}",
            t.id,
            t.chain,
            t.state,
            t.score,
            trunc(&t.name, 16),
            t.address
        );
    }
    Ok(())
}

fn cmd_export() -> Result<()> {
    let conn = db::open(&db_path())?;
    let n = db::export_csv(&conn, &csv_path())?;
    println!("{n} rows → {}", csv_path().display());
    Ok(())
}

fn cmd_open(id: i64) -> Result<()> {
    let conn = db::open(&db_path())?;
    let t = db::get(&conn, id)?;
    let dir = hunts_dir().join(format!("{id}-{}", sanitize(&t.name)));
    fs::create_dir_all(dir.join("poc/src"))?;
    fs::create_dir_all(dir.join("poc/test"))?;
    write_if_missing(&dir.join("NOTES.md"), &notes_md(&t))?;
    write_if_missing(&dir.join("REPORT.md"), &report_md(&t))?;
    write_if_missing(&dir.join("poc/foundry.toml"), FOUNDRY_TOML)?;
    write_if_missing(&dir.join("poc/test/Hypothesis.t.sol"), POC_SOL)?;
    db::set_state(&conn, id, "scoped")?;
    println!("{}", dir.display());
    println!("state → scoped");
    Ok(())
}

fn cmd_report(id: i64) -> Result<()> {
    let conn = db::open(&db_path())?;
    let t = db::get(&conn, id)?;
    let path = hunts_dir()
        .join(format!("{id}-{}", sanitize(&t.name)))
        .join("REPORT.md");
    if path.exists() {
        print!("{}", fs::read_to_string(&path)?);
    } else {
        print!("{}", report_md(&t));
    }
    db::set_state(&conn, id, "reported")?;
    eprintln!("\nstate → reported  (send privately)");
    Ok(())
}

fn cmd_state(id: i64, state: String) -> Result<()> {
    let allowed = [
        "seen",
        "scored",
        "scoped",
        "hypothesized",
        "proven",
        "killed",
        "reported",
        "patched",
    ];
    if !allowed.contains(&state.as_str()) {
        anyhow::bail!("state must be one of: {}", allowed.join("|"));
    }
    let conn = db::open(&db_path())?;
    db::set_state(&conn, id, &state)?;
    println!("#{id} → {state}");
    Ok(())
}

fn sanitize(s: &str) -> String {
    let s: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    if s.is_empty() {
        "target".into()
    } else {
        s
    }
}

fn trunc(s: &str, n: usize) -> String {
    let mut it = s.chars();
    let mut out = String::new();
    for _ in 0..n {
        match it.next() {
            Some(c) => out.push(c),
            None => break,
        }
    }
    out
}

fn write_if_missing(path: &Path, body: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    let mut f = fs::File::create(path).with_context(|| path.display().to_string())?;
    f.write_all(body.as_bytes())?;
    Ok(())
}

fn notes_md(t: &db::Target) -> String {
    format!(
        r#"# {name}

- id: {id}
- chain: {chain}
- CA: `{addr}`
- pair: `{pair}`
- url: {url}
- source: {source}
- first_seen: {seen}
- score: {score}
- signals: {signals}

## Surface
- [ ] verified source pulled (explorer, not github)
- [ ] proxy / implementation / admin mapped
- [ ] functions that move value listed

## Duke questions
1. Who is allowed to move the money?
2. What happens at the handoff between contracts?
3. Can a stranger trigger something only the team should?

## Hypotheses (max 3)
- [ ] H1:
- [ ] H2:
- [ ] H3:

## Kill / prove
- test: `poc/test/Hypothesis.t.sol`
- result:

## Decision
- [ ] killed — reason:
- [ ] proven — report privately
"#,
        name = t.name,
        id = t.id,
        chain = t.chain,
        addr = t.address,
        pair = t.pair,
        url = t.url,
        source = t.source,
        seen = t.first_seen,
        score = t.score,
        signals = t.signals,
    )
}

fn report_md(t: &db::Target) -> String {
    format!(
        r#"# {name} — draft report

**Do not post this while the bug is open.**
Send privately. Best line: don't trust me; run this yourself.

- chain: {chain}
- address: `{addr}`
- url: {url}
- severity: (honest — do not inflate)
- impact:

## Vulnerable code
- file / line:

## PoC
```
cd poc
forge test --fork-url $RPC --match-test testHypothesis -vvv
```

## Fix
(one paragraph)
"#,
        name = t.name,
        chain = t.chain,
        addr = t.address,
        url = t.url,
    )
}

const FOUNDRY_TOML: &str = r#"[profile.default]
src = "src"
out = "out"
libs = ["lib"]
solc = "0.8.24"
evm_version = "cancun"
"#;

const POC_SOL: &str = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {console2} from "forge-std/console2.sol";

contract HypothesisTest is Test {
    address attacker = makeAddr("attacker");

    function setUp() public {
        deal(attacker, 10 ether);
    }

    function testHypothesis() public {
        vm.startPrank(attacker);
        console2.log("replace this with a real assertion against the CA");
        vm.stopPrank();
    }
}
"#;
