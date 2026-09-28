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
    Init,
    Pull {
        #[arg(long)]
        query: Option<String>,
        #[arg(long, default_value_t = 25)]
        limit: usize,
        #[arg(long, default_value = "dex")]
        source: String,
        #[arg(long)]
        demo: bool,
        #[arg(long)]
        address: Option<String>,
        #[arg(long, default_value = "base")]
        chain: String,
        #[arg(long, default_value = "unnamed")]
        name: String,
        #[arg(long, default_value = "")]
        deployer: String,
    },
    List,
    Export,
    Open { id: i64 },
    Report { id: i64 },
    State { id: i64, state: String },
}

fn root() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".scout")
}
fn db_path() -> PathBuf { root().join("scout.db") }
fn hunts_dir() -> PathBuf { root().join("hunts") }
fn csv_path() -> PathBuf { root().join("cas.csv") }

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Init => cmd_init(),
        Cmd::Pull { query, limit, source, demo, address, chain, name, deployer } => {
            cmd_pull(query, limit, source, demo, address, chain, name, deployer)
        }
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
    println!("scout ready\n  db    {}\n  csv   {}\n  hunts {}\n\nnext: scout pull", db_path().display(), csv_path().display(), hunts_dir().display());
    Ok(())
}

fn cmd_pull(query: Option<String>, limit: usize, source: String, demo: bool, address: Option<String>, chain: String, name: String, deployer: String) -> Result<()> {
    fs::create_dir_all(hunts_dir())?;
    let conn = db::open(&db_path())?;
    if let Some(addr) = address {
        let id = db::upsert(&conn, &chain, &addr, &name, &deployer, "", "", "manual", 4, &["manual".into()])?;
        println!("#{id}  {chain}  {addr}");
        let n = db::export_csv(&conn, &csv_path())?;
        println!("saved {n} CAs -> {}", csv_path().display());
        return Ok(());
    }
    if demo {
        db::upsert(&conn, "base", "0x1111111111111111111111111111111111111111", "FreshVault", "", "", "", "demo", 3, &["demo".into()])?;
        let n = db::export_csv(&conn, &csv_path())?;
        println!("demo queue. saved {n} CAs -> {}", csv_path().display());
        return Ok(());
    }
    let found = match source.as_str() {
        "sourcify" => {
            let id = ingest::chain_id_of(&chain);
            println!("pulling sourcify verified contracts on {chain} ({id})");
            ingest::sourcify_latest(id, limit)?
        }
        "all" => {
            let mut out = Vec::new();
            if let Some(q) = query.clone() {
                println!("searching dexscreener for `{q}`");
                out.extend(ingest::search(&q, limit)?);
            } else {
                out.extend(ingest::latest_profiles(limit)?);
            }
            let id = ingest::chain_id_of(&chain);
            out.extend(ingest::sourcify_latest(id, limit)?);
            out
        }
        _ => {
            if let Some(q) = query {
                println!("searching dexscreener for `{q}`");
                ingest::search(&q, limit)?
            } else {
                ingest::latest_profiles(limit)?
            }
        }
    };
    if found.is_empty() {
        println!("nothing returned. try: scout pull --query base");
        return Ok(());
    }
    for f in &found {
        let id = db::upsert(&conn, &f.chain, &f.address, &f.name, "", &f.pair, &f.url, &f.source, f.score, &f.signals)?;
        println!("#{:<4} {:<10} {:<16} {}", id, f.chain, trunc(&f.name, 16), f.address);
    }
    let n = db::export_csv(&conn, &csv_path())?;
    println!("\n{n} CAs in db + {}", csv_path().display());
    Ok(())
}

fn cmd_list() -> Result<()> {
    let conn = db::open(&db_path())?;
    let rows = db::list(&conn)?;
    if rows.is_empty() { println!("empty. run: scout pull"); return Ok(()); }
    println!("{:>4}  {:<10} {:<8} {:>5}  {:<16} {}", "id", "chain", "state", "score", "name", "address");
    for t in rows {
        println!("{:>4}  {:<10} {:<8} {:>5}  {:<16} {}", t.id, t.chain, t.state, t.score, trunc(&t.name, 16), t.address);
    }
    Ok(())
}

fn cmd_export() -> Result<()> {
    let n = db::export_csv(&db::open(&db_path())?, &csv_path())?;
    println!("{n} rows -> {}", csv_path().display());
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
    write_if_missing(&dir.join("poc/foundry.toml"), "[profile.default]\nsrc = \"src\"\nout = \"out\"\nlibs = [\"lib\"]\n")?;
    write_if_missing(&dir.join("poc/test/Hypothesis.t.sol"), POC_SOL)?;
    db::set_state(&conn, id, "scoped")?;
    println!("{}", dir.display());
    Ok(())
}

fn cmd_report(id: i64) -> Result<()> {
    let conn = db::open(&db_path())?;
    let t = db::get(&conn, id)?;
    let path = hunts_dir().join(format!("{id}-{}", sanitize(&t.name))).join("REPORT.md");
    if path.exists() { print!("{}", fs::read_to_string(&path)?); } else { print!("{}", report_md(&t)); }
    db::set_state(&conn, id, "reported")?;
    eprintln!("state -> reported (send privately)");
    Ok(())
}

fn cmd_state(id: i64, state: String) -> Result<()> {
    let allowed = ["seen","scored","scoped","hypothesized","proven","killed","reported","patched"];
    if !allowed.contains(&state.as_str()) { anyhow::bail!("bad state"); }
    db::set_state(&db::open(&db_path())?, id, &state)?;
    println!("#{id} -> {state}");
    Ok(())
}

fn sanitize(s: &str) -> String {
    let s: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    if s.is_empty() { "target".into() } else { s }
}
fn trunc(s: &str, n: usize) -> String { s.chars().take(n).collect() }
fn write_if_missing(path: &Path, body: &str) -> Result<()> {
    if path.exists() { return Ok(()); }
    if let Some(p) = path.parent() { fs::create_dir_all(p)?; }
    fs::write(path, body)?;
    Ok(())
}
fn notes_md(t: &db::Target) -> String {
    format!("# {name}\n\n- id: {id}\n- chain: {chain}\n- CA: `{addr}`\n- url: {url}\n- source: {source}\n- score: {score}\n- signals: {signals}\n\n## Duke questions\n1. Who is allowed to move the money?\n2. What happens at the handoff between contracts?\n3. Can a stranger trigger something only the team should?\n\n## Hypotheses\n- [ ] H1:\n- [ ] H2:\n- [ ] H3:\n", name=t.name, id=t.id, chain=t.chain, addr=t.address, url=t.url, source=t.source, score=t.score, signals=t.signals)
}
fn report_md(t: &db::Target) -> String {
    format!("# {name} draft\n\nDo not post while open.\n- chain: {chain}\n- address: `{addr}`\n- url: {url}\n- severity:\n- impact:\n", name=t.name, chain=t.chain, addr=t.address, url=t.url)
}
const POC_SOL: &str = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.24;\nimport {Test} from \"forge-std/Test.sol\";\ncontract HypothesisTest is Test { function testHypothesis() public {} }\n";
