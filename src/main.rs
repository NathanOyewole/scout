use anyhow::Result;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

mod db;
mod ingest;
mod source;

#[derive(Parser)]
#[command(name = "scout", about = "Find CAs. Pull source. Hunt offline.", version)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    Init,
    Pull {
        #[arg(long)] query: Option<String>,
        #[arg(long, default_value_t = 25)] limit: usize,
        #[arg(long, default_value = "dex")] source: String,
        #[arg(long)] demo: bool,
        #[arg(long)] address: Option<String>,
        #[arg(long, default_value = "base")] chain: String,
        #[arg(long, default_value = "unnamed")] name: String,
        #[arg(long, default_value = "")] deployer: String,
    },
    List,
    Export,
    Open { id: i64 },
    Source { id: i64 },
    Report { id: i64 },
    State { id: i64, state: String },
}

fn root() -> PathBuf { dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".scout") }
fn db_path() -> PathBuf { root().join("scout.db") }
fn hunts_dir() -> PathBuf { root().join("hunts") }
fn csv_path() -> PathBuf { root().join("cas.csv") }

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Init => cmd_init(),
        Cmd::Pull { query, limit, source, demo, address, chain, name, deployer } => cmd_pull(query, limit, source, demo, address, chain, name, deployer),
        Cmd::List => cmd_list(),
        Cmd::Export => cmd_export(),
        Cmd::Open { id } => cmd_open(id),
        Cmd::Source { id } => cmd_source(id),
        Cmd::Report { id } => cmd_report(id),
        Cmd::State { id, state } => cmd_state(id, state),
    }
}

fn cmd_init() -> Result<()> {
    fs::create_dir_all(hunts_dir())?;
    let _ = db::open(&db_path())?;
    println!("scout ready\n  {}\nnext: scout pull --source sourcify --chain base", db_path().display());
    Ok(())
}

fn cmd_pull(query: Option<String>, limit: usize, source_kind: String, demo: bool, address: Option<String>, chain: String, name: String, deployer: String) -> Result<()> {
    fs::create_dir_all(hunts_dir())?;
    let conn = db::open(&db_path())?;
    if let Some(addr) = address {
        let id = db::upsert(&conn, &chain, &addr, &name, &deployer, "", "", "manual", 4, &["manual".into()])?;
        println!("#{id} {chain} {addr}");
        db::export_csv(&conn, &csv_path())?;
        return Ok(());
    }
    if demo {
        db::upsert(&conn, "base", "0x1111111111111111111111111111111111111111", "FreshVault", "", "", "", "demo", 3, &["demo".into()])?;
        db::export_csv(&conn, &csv_path())?;
        println!("demo queued");
        return Ok(());
    }
    let found = match source_kind.as_str() {
        "sourcify" => ingest::sourcify_latest(ingest::chain_id_of(&chain), limit)?,
        "all" => {
            let mut out = if let Some(q) = query.clone() { ingest::search(&q, limit)? } else { ingest::latest_profiles(limit)? };
            out.extend(ingest::sourcify_latest(ingest::chain_id_of(&chain), limit)?);
            out
        }
        _ => { if let Some(q) = query { ingest::search(&q, limit)? } else { ingest::latest_profiles(limit)? } }
    };
    if found.is_empty() { println!("empty. try: scout pull --source sourcify --chain base"); return Ok(()); }
    for f in &found {
        let id = db::upsert(&conn, &f.chain, &f.address, &f.name, "", &f.pair, &f.url, &f.source, f.score, &f.signals)?;
        println!("#{id} {} {} {}", f.chain, f.name, f.address);
    }
    let n = db::export_csv(&conn, &csv_path())?;
    println!("{n} CAs -> {}\nnext: scout source <id>", csv_path().display());
    Ok(())
}

fn cmd_list() -> Result<()> {
    let rows = db::list(&db::open(&db_path())?)?;
    if rows.is_empty() { println!("empty. run: scout pull --source sourcify --chain base"); return Ok(()); }
    println!("{:>4} {:<10} {:<8} {:>5} {:<18} {}", "id", "chain", "state", "score", "name", "address");
    for t in rows {
        println!("{:>4} {:<10} {:<8} {:>5} {:<18} {}", t.id, t.chain, t.state, t.score, trunc(&t.name, 18), t.address);
    }
    Ok(())
}

fn cmd_export() -> Result<()> {
    let n = db::export_csv(&db::open(&db_path())?, &csv_path())?;
    println!("{n} -> {}", csv_path().display());
    Ok(())
}

fn hunt_dir(t: &db::Target) -> PathBuf { hunts_dir().join(format!("{}-{}", t.id, sanitize(&t.name))) }

fn cmd_open(id: i64) -> Result<()> {
    let conn = db::open(&db_path())?;
    let t = db::get(&conn, id)?;
    let dir = hunt_dir(&t);
    fs::create_dir_all(dir.join("poc/test"))?;
    write_if_missing(&dir.join("NOTES.md"), &notes_md(&t))?;
    write_if_missing(&dir.join("REPORT.md"), &report_md(&t))?;
    write_if_missing(&dir.join("poc/foundry.toml"), "[profile.default]\nsrc = \"src\"\nout = \"out\"\nlibs = [\"lib\"]\n")?;
    write_if_missing(&dir.join("poc/test/Hypothesis.t.sol"), POC_SOL)?;
    db::set_state(&conn, id, "scoped")?;
    println!("{}", dir.display());
    Ok(())
}

fn cmd_source(id: i64) -> Result<()> {
    let conn = db::open(&db_path())?;
    let t = db::get(&conn, id)?;
    let dir = hunt_dir(&t);
    if !dir.join("NOTES.md").exists() { cmd_open(id)?; }
    println!("fetch {} / {}", t.chain, t.address);
    source::fetch_into(&dir, &t.chain, &t.address)?;
    db::set_state(&conn, id, "scoped")?;
    Ok(())
}

fn cmd_report(id: i64) -> Result<()> {
    let conn = db::open(&db_path())?;
    let t = db::get(&conn, id)?;
    let path = hunt_dir(&t).join("REPORT.md");
    if path.exists() { print!("{}", fs::read_to_string(&path)?); } else { print!("{}", report_md(&t)); }
    db::set_state(&conn, id, "reported")?;
    eprintln!("send privately");
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
    fs::write(path, body)?; Ok(())
}
fn notes_md(t: &db::Target) -> String {
    format!("# {name}\n\n- id: {id}\n- chain: {chain}\n- CA: `{addr}`\n- url: {url}\n- source: {source}\n- score: {score}\n- signals: {signals}\n\n## Duke questions\n1. Who is allowed to move the money?\n2. What happens at the handoff between contracts?\n3. Can a stranger trigger something only the team should?\n\n## Hypotheses\n- [ ] H1:\n- [ ] H2:\n- [ ] H3:\n", name=t.name, id=t.id, chain=t.chain, addr=t.address, url=t.url, source=t.source, score=t.score, signals=t.signals)
}
fn report_md(t: &db::Target) -> String {
    format!("# {name} draft\n\nDo not post while open.\n- chain: {chain}\n- address: `{addr}`\n- url: {url}\n- severity:\n- impact:\n", name=t.name, chain=t.chain, addr=t.address, url=t.url)
}
const POC_SOL: &str = "// SPDX-License-Identifier: MIT\npragma solidity ^0.8.24;\nimport {Test} from \"forge-std/Test.sol\";\ncontract HypothesisTest is Test { function testHypothesis() public {} }\n";
