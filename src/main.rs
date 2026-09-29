use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

mod db;
mod ingest;
mod poc;
mod source;

#[derive(Parser)]
#[command(
    name = "scout",
    about = "Find CAs. Pull source. Prove on a fork.",
    version
)]
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
        #[arg(long, default_value = "indie")]
        kind: String,
        #[arg(long, default_value = "")]
        program: String,
    },
    List {
        #[arg(long, default_value = "all")]
        kind: String,
        #[arg(long, default_value_t = 40)]
        limit: usize,
    },
    Export,
    Open {
        id: i64,
    },
    Source {
        id: i64,
    },
    Test {
        id: i64,
    },
    Prove {
        id: i64,
    },
    Rate {
        id: i64,
        level: String,
        #[arg(long, default_value = "")]
        note: String,
    },
    Report {
        id: i64,
    },
    State {
        id: i64,
        state: String,
    },
    /// Drop a target and its hunt folder. Refuses while a severity is set,
    /// so a graded hypothesis cannot be deleted by accident.
    Rm {
        id: i64,
        #[arg(long)]
        force: bool,
    },
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

fn conn() -> Result<Connection> {
    fs::create_dir_all(hunts_dir())?;
    db::open(&db_path())
}

/// cas.csv is what you paste into a submission, so it is rewritten after every
/// mutation instead of only after a pull. It used to go stale: the db said
/// #27 was scoped while the csv still said seen.
fn export(conn: &Connection) {
    if let Err(e) = db::export_csv(conn, &csv_path()) {
        eprintln!("warning: could not refresh {}: {e}", csv_path().display());
    }
}

/// A failure carrying a specific process exit code. Used for 127 ("forge is
/// not installed"), which is the shell's command-not-found convention and is
/// what the README promises. anyhow's default would be 1.
#[derive(Debug)]
struct Coded(i32, anyhow::Error);

impl std::fmt::Display for Coded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.1)
    }
}

impl std::error::Error for Coded {}

fn coded(code: i32, msg: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(Coded(code, anyhow::anyhow!(msg.into())))
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            match e.downcast_ref::<Coded>() {
                Some(c) => std::process::ExitCode::from(c.0 as u8),
                None => std::process::ExitCode::FAILURE,
            }
        }
    }
}

fn run() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Init => cmd_init(),
        Cmd::Pull {
            query,
            limit,
            source,
            demo,
            address,
            chain,
            name,
            deployer,
            kind,
            program,
        } => cmd_pull(
            query, limit, source, demo, address, chain, name, deployer, kind, program,
        ),
        Cmd::List { kind, limit } => cmd_list(kind, limit),
        Cmd::Export => cmd_export(),
        Cmd::Open { id } => cmd_open(id),
        Cmd::Source { id } => cmd_source(id),
        Cmd::Test { id } => cmd_test(id),
        Cmd::Prove { id } => cmd_prove(id),
        Cmd::Rate { id, level, note } => cmd_rate(id, level, note),
        Cmd::Report { id } => cmd_report(id),
        Cmd::State { id, state } => cmd_state(id, state),
        Cmd::Rm { id, force } => cmd_rm(id, force),
    }
}

fn cmd_init() -> Result<()> {
    let c = conn()?;
    let n = db::list(&c)?.len();
    println!("scout ready\n  {}\n  {} target(s)", db_path().display(), n);
    Ok(())
}

/// The origin of a find (sourcify / dexscreener / manual) stays in `source`.
/// The program it belongs to goes in signals, so `list --kind bounty` still
/// finds it. The old code overwrote source with "bounty:immunefi" and lost
/// the radar the target actually came from.
fn tag_signals(base: &[String], kind: &str, program: &str) -> Vec<String> {
    let mut sigs = base.to_vec();
    let k = kind.trim().to_ascii_lowercase();
    if !k.is_empty() && !sigs.iter().any(|s| s.eq_ignore_ascii_case(&k)) {
        sigs.push(k);
    }
    if !program.trim().is_empty() && !sigs.iter().any(|s| s.eq_ignore_ascii_case(program)) {
        sigs.push(program.trim().to_string());
    }
    sigs
}

#[allow(clippy::too_many_arguments)]
fn cmd_pull(
    query: Option<String>,
    limit: usize,
    source_kind: String,
    demo: bool,
    address: Option<String>,
    chain: String,
    name: String,
    deployer: String,
    kind: String,
    program: String,
) -> Result<()> {
    let c = conn()?;
    if let Some(addr) = address {
        let addr = addr.trim().to_string();
        if !poc::is_hex_address(&addr) {
            anyhow::bail!("'{addr}' is not a 20-byte hex CA (0x + 40 hex chars). got: {addr}");
        }
        let chain_id = ingest::chain_id(&chain).with_context_chain(&chain)?;
        let _ = chain_id;
        let url = ingest::explorer_url(&chain, &addr);
        let sigs = tag_signals(&[], &kind, &program);
        let id = db::upsert(
            &c, &chain, &addr, &name, &deployer, "", &url, "manual", 4, &sigs,
        )?;
        println!("#{id} manual {chain} {addr}\n  {url}");
        export(&c);
        return Ok(());
    }
    if demo {
        db::upsert(
            &c,
            "base",
            "0x1111111111111111111111111111111111111111",
            "FreshVault",
            "",
            "",
            "",
            "demo",
            3,
            &["demo".into()],
        )?;
        export(&c);
        println!("demo queued");
        return Ok(());
    }
    let found = match source_kind.as_str() {
        "sourcify" => {
            let id = ingest::chain_id(&chain).with_context_chain(&chain)?;
            ingest::sourcify_latest(id, limit)?
        }
        "all" => {
            let mut out = if let Some(q) = query.clone() {
                ingest::search(&q, limit)?
            } else {
                ingest::latest_profiles(limit)?
            };
            if let Some(id) = ingest::chain_id(&chain) {
                out.extend(ingest::sourcify_latest(id, limit)?);
            } else {
                eprintln!("note: '{chain}' is not an EVM chain — skipping the sourcify radar");
            }
            out
        }
        _ => {
            if let Some(q) = query {
                ingest::search(&q, limit)?
            } else {
                ingest::latest_profiles(limit)?
            }
        }
    };
    if found.is_empty() {
        println!("empty");
        return Ok(());
    }
    let mut non_evm = 0usize;
    for f in &found {
        if ingest::chain_id(&f.chain).is_none() {
            non_evm += 1;
            continue;
        }
        let sigs = tag_signals(&f.signals, &kind, &program);
        let id = db::upsert(
            &c, &f.chain, &f.address, &f.name, "", &f.pair, &f.url, &f.source, f.score, &sigs,
        )?;
        println!("#{id} {} {} {}", f.chain, f.name, f.address);
    }
    if non_evm > 0 {
        println!("skipped {non_evm} non-EVM find(s) — scout reads EVM source only");
    }
    export(&c);
    Ok(())
}

trait ChainCtx {
    fn with_context_chain(self, chain: &str) -> Result<u64>;
}
impl ChainCtx for Option<u64> {
    fn with_context_chain(self, chain: &str) -> Result<u64> {
        self.ok_or_else(|| anyhow::anyhow!("'{chain}' is not an EVM chain. try: base, ethereum, arbitrum, optimism, bsc, polygon, avalanche, linea, scroll"))
    }
}

fn cmd_list(kind: String, limit: usize) -> Result<()> {
    let c = conn()?;
    let filter = match kind.as_str() {
        "bounty" | "indie" => Some(kind.as_str()),
        _ => None,
    };
    let rows = db::list_kind(&c, filter)?;
    if rows.is_empty() {
        println!("empty");
        return Ok(());
    }
    println!(
        "{:>4} {:<9} {:<8} {:<12} {:<6} {:<9} {:<18} address",
        "id", "chain", "score", "state", "heat", "severity", "name"
    );
    for t in rows.iter().take(limit) {
        println!(
            "{:>4} {:<9} {:<8} {:<12} {:<6} {:<9} {:<18} {}",
            t.id,
            t.chain,
            t.score,
            t.state,
            dash(&t.heat),
            dash(&t.severity),
            trunc(&label(t), 18),
            t.address
        );
    }
    if rows.len() > limit {
        println!("… {} more (raise --limit)", rows.len() - limit);
    }
    Ok(())
}

fn label(t: &db::Target) -> String {
    if t.contract.is_empty() {
        t.name.clone()
    } else {
        t.contract.clone()
    }
}
fn dash(s: &str) -> &str {
    if s.is_empty() {
        "-"
    } else {
        s
    }
}

fn cmd_export() -> Result<()> {
    let c = conn()?;
    let n = db::export_csv(&c, &csv_path())?;
    println!("{n} -> {}", csv_path().display());
    Ok(())
}

fn hunt_dir(t: &db::Target) -> PathBuf {
    // Derived from the name at insert time and never rewritten, so an in-flight
    // hunt folder can never move out from under you.
    hunts_dir().join(format!("{}-{}", t.id, sanitize(&t.name)))
}

fn scaffold(dir: &Path, t: &db::Target) -> Result<()> {
    write_if_missing(&dir.join("NOTES.md"), &notes_md(t))?;
    write_if_missing(&dir.join("REPORT.md"), &report_md(t))?;
    write_if_missing(
        &dir.join("poc/foundry.toml"),
        "[profile.default]\nsrc = \"src\"\nout = \"out\"\nlibs = [\"lib\"]\n\n# fork only. never --broadcast.\n",
    )?;
    write_if_missing(
        &dir.join("poc/src/ITarget.keep"),
        "// TargetCalls.sol is generated by `scout source`.\n",
    )?;
    write_if_missing(
        &dir.join("poc/test/Hypothesis.t.sol"),
        &poc::fork_test_sol(t),
    )?;
    Ok(())
}

fn ensure(dir: &Path, t: &db::Target) -> Result<()> {
    fs::create_dir_all(dir.join("poc/test"))?;
    scaffold(dir, t)
}

/// Everything a per-target command needs. Six commands repeated the same
/// conn/get/hunt_dir/ensure prologue, and each had to remember to call
/// `export` at the end — which is exactly how cas.csv went stale before.
///
/// The Drop impl is the fix: any command holding a Hunt refreshes the
/// export when it returns, so a new mutating command cannot forget to.
struct Hunt {
    conn: Connection,
    target: db::Target,
    dir: PathBuf,
}

impl Hunt {
    fn open(id: i64) -> Result<Self> {
        let conn = conn()?;
        let target = db::get(&conn, id)?;
        let dir = hunt_dir(&target);
        Ok(Self { conn, target, dir })
    }

    /// Create the hunt folder and its scaffold if they are not there yet.
    fn ensure(&self) -> Result<()> {
        ensure(&self.dir, &self.target)
    }
}

impl Drop for Hunt {
    fn drop(&mut self) {
        export(&self.conn);
    }
}

fn cmd_open(id: i64) -> Result<()> {
    let h = Hunt::open(id)?;
    h.ensure()?;
    db::set_state(&h.conn, id, "scoped")?;
    println!("{}", h.dir.display());
    Ok(())
}

fn cmd_source(id: i64) -> Result<()> {
    let h = Hunt::open(id)?;
    h.ensure()?;
    let (c, t) = (&h.conn, &h.target);
    println!("fetch {} / {}", t.chain, t.address);
    let fetched = source::fetch_into(&h.dir, &t.chain, &t.address)?;
    if !fetched.contract_name.is_empty() {
        db::set_field(c, id, "contract", &fetched.contract_name)?;
    }
    if !fetched.deployer.is_empty() {
        db::set_field(c, id, "deployer", &fetched.deployer)?;
    }
    if !fetched.kind.is_empty() {
        let mut sigs: Vec<String> = serde_json::from_str(&t.signals).unwrap_or_default();
        for k in [
            format!("kind:{}", fetched.kind),
            format!("match:{}", fetched.match_kind),
        ] {
            if !k.is_empty() && !sigs.contains(&k) {
                sigs.push(k);
            }
        }
        db::set_field(c, id, "signals", &serde_json::to_string(&sigs)?)?;
    }
    db::set_field(c, id, "heat", &fetched.heat)?;
    if fetched.heat == "skip" {
        db::set_state(c, id, "killed")?;
        println!("state -> killed (heat=skip)");
    } else if t.state == "seen" || t.state == "killed" {
        db::set_state(c, id, "scoped")?;
    }
    Ok(())
}

fn cmd_test(id: i64) -> Result<()> {
    let h = Hunt::open(id)?;
    h.ensure()?;
    poc::print_test_cmds(&h.dir, &h.target)?;
    db::set_state(&h.conn, id, "hypothesized")?;
    Ok(())
}

fn cmd_prove(id: i64) -> Result<()> {
    let h = Hunt::open(id)?;
    h.ensure()?;
    let still_stub = poc::hypothesis_is_placeholder(&h.dir);
    let code = poc::prove(&h.dir, &h.target)?;
    match code {
        0 if still_stub => {
            // The old code set state -> proven here. A scaffold with an empty
            // testHypothesis passes, so this is how an unproven hypothesis gets
            // reported. Never mark proven off a stub.
            db::set_state(&h.conn, id, "hypothesized")?;
            println!();
            println!(
                "scaffold compiles and the fork is reachable, but testHypothesis is still empty."
            );
            println!(
                "that proves nothing. write the hypothesis in {} and run it again.",
                h.dir.join("poc/test/Hypothesis.t.sol").display()
            );
            println!("state stays hypothesized (not proven).");
        }
        0 => {
            db::set_state(&h.conn, id, "proven")?;
            println!(
                "state -> proven (fork test passed against {})",
                h.target.address
            );
            println!("now: scout rate {id} <level> --note \"...\"");
        }
        127 => {
            return Err(coded(
                127,
                format!("forge is not installed - install it, then: scout prove {id}"),
            ))
        }
        other => println!("forge exited {other} — the test did not pass, so nothing is proven"),
    }
    Ok(())
}

fn cmd_rate(id: i64, level: String, note: String) -> Result<()> {
    const ALLOWED: [&str; 5] = ["info", "low", "medium", "high", "critical"];
    let level = level.trim().to_ascii_lowercase();
    if !ALLOWED.contains(&level.as_str()) {
        anyhow::bail!("rate = {}", ALLOWED.join("|"));
    }
    let h = Hunt::open(id)?;
    h.ensure()?;
    source::write_rate(&h.dir, &level, &note)?;
    db::set_field(&h.conn, id, "severity", &level)?;
    println!(
        "#{id} severity {level} -> {}",
        h.dir.join("SEVERITY.md").display()
    );
    if !note.trim().is_empty() {
        println!("note: {}", note.trim());
    }
    Ok(())
}

fn cmd_report(id: i64) -> Result<()> {
    let h = Hunt::open(id)?;
    let path = h.dir.join("REPORT.md");
    let has_report = path.exists();
    if has_report {
        print!("{}", fs::read_to_string(&path)?);
    } else {
        print!("{}", report_md(&h.target));
    }
    // Do not let a blank draft count as a submission.
    if h.target.severity.is_empty() {
        eprintln!();
        eprintln!(
            "no severity set — state stays {}. run: scout rate {id} <level> --note \"...\"",
            h.target.state
        );
        return Ok(());
    }
    if !has_report {
        eprintln!();
        eprintln!("no REPORT.md on disk — printed the template, state unchanged.");
        return Ok(());
    }
    db::set_state(&h.conn, id, "reported")?;
    eprintln!();
    eprintln!("state -> reported. send it privately. do not post while open.");
    Ok(())
}

fn cmd_state(id: i64, state: String) -> Result<()> {
    let state = state.trim().to_ascii_lowercase();
    let c = conn()?;
    db::get(&c, id)?;
    db::set_state(&c, id, &state)?;
    export(&c);
    println!("#{id} -> {state}");
    Ok(())
}

fn cmd_rm(id: i64, force: bool) -> Result<()> {
    let h = Hunt::open(id)?;
    if !h.target.severity.is_empty() && !force {
        anyhow::bail!(
            "#{id} is graded '{}' — pass --force if you really mean it",
            h.target.severity
        );
    }
    db::delete(&h.conn, id)?;
    let removed = if h.dir.exists() {
        fs::remove_dir_all(&h.dir).with_context(|| format!("remove {}", h.dir.display()))?;
        "row + hunt folder"
    } else {
        "row"
    };
    println!("#{id} gone ({removed})");
    Ok(())
}

fn sanitize(s: &str) -> String {
    let s: String = s
        .chars()
        .take(48)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "target".into()
    } else {
        s
    }
}
fn trunc(s: &str, n: usize) -> String {
    let t: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        format!("{t}…")
    } else {
        t
    }
}
fn write_if_missing(path: &Path, body: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(p) = path.parent() {
        fs::create_dir_all(p)?;
    }
    fs::write(path, body)?;
    Ok(())
}

fn notes_md(t: &db::Target) -> String {
    format!(
        "# {name}\n\n- id: {id}\n- chain: {chain}\n- CA: `{addr}`\n- url: {url}\n- source: {source}\n- score: {score}\n- signals: {signals}\n\n## Duke questions\n1. Who is allowed to move the money?\n2. What happens at the handoff between contracts?\n3. Can a stranger trigger something only the team should?\n\n## Hypotheses\n- [ ] H1:\n- [ ] H2:\n- [ ] H3:\n",
        name = t.name,
        id = t.id,
        chain = t.chain,
        addr = t.address,
        url = t.url,
        source = t.source,
        score = t.score,
        signals = t.signals
    )
}

fn report_md(t: &db::Target) -> String {
    format!(
        "# {name} draft\n\nDo not post while open.\n- chain: {chain}\n- address: `{addr}`\n- url: {url}\n- source: {source}\n- deployer: `{deployer}`\n- severity:\n- impact:\n- root cause:\n- proof: scout prove {id} (fork test, no broadcast)\n- remediation:\n",
        name = t.name,
        chain = t.chain,
        addr = t.address,
        url = t.url,
        source = t.source,
        deployer = t.deployer,
        id = t.id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_is_path_safe_and_bounded() {
        assert_eq!(sanitize("YieldSeeker"), "YieldSeeker");
        assert_eq!(sanitize("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitize("a/b\\c"), "a-b-c");
        assert_eq!(sanitize("   "), "target");
        assert_eq!(sanitize(""), "target");
        assert!(sanitize(&"x".repeat(500)).len() <= 48);
        assert!(!sanitize(&"x".repeat(500)).contains('/'));
    }

    #[test]
    fn trunc_marks_that_it_cut() {
        assert_eq!(trunc("abc", 5), "abc");
        assert_eq!(trunc("abcdef", 3), "abc…");
    }

    #[test]
    fn tag_signals_keeps_origin_and_adds_queue_once() {
        let s = tag_signals(&["sourcify".into(), "verified".into()], "indie", "");
        assert_eq!(s, vec!["sourcify", "verified", "indie"]);
        let s2 = tag_signals(&["dexscreener".into()], "bounty", "immunefi");
        assert_eq!(s2, vec!["dexscreener", "bounty", "immunefi"]);
        let s3 = tag_signals(&["sourcify".into()], "bounty", "sourcify");
        assert_eq!(s3, vec!["sourcify", "bounty"], "no duplicate entries");
    }
}
