use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Target {
    pub id: i64,
    pub chain: String,
    pub address: String,
    pub name: String,
    pub deployer: String,
    pub pair: String,
    pub url: String,
    pub source: String,
    pub first_seen: String,
    pub score: i64,
    pub state: String,
    pub signals: String,
}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() { std::fs::create_dir_all(parent)?; }
    let conn = Connection::open(path).context("open sqlite")?;
    conn.execute_batch(r#"
        PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS targets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            chain TEXT NOT NULL,
            address TEXT NOT NULL,
            name TEXT NOT NULL DEFAULT '',
            deployer TEXT NOT NULL DEFAULT '',
            pair TEXT NOT NULL DEFAULT '',
            url TEXT NOT NULL DEFAULT '',
            source TEXT NOT NULL DEFAULT '',
            first_seen TEXT NOT NULL,
            score INTEGER NOT NULL DEFAULT 0,
            state TEXT NOT NULL DEFAULT 'seen',
            signals TEXT NOT NULL DEFAULT '[]',
            UNIQUE(chain, address)
        );
    "#)?;
    let _ = conn.execute_batch(
        "ALTER TABLE targets ADD COLUMN pair TEXT NOT NULL DEFAULT '';
         ALTER TABLE targets ADD COLUMN url TEXT NOT NULL DEFAULT '';
         ALTER TABLE targets ADD COLUMN source TEXT NOT NULL DEFAULT '';",
    );
    Ok(conn)
}

pub fn upsert(conn: &Connection, chain: &str, address: &str, name: &str, deployer: &str, pair: &str, url: &str, source: &str, score: i64, signals: &[String]) -> Result<i64> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let sig = serde_json::to_string(signals)?;
    conn.execute(
        r#"INSERT INTO targets (chain, address, name, deployer, pair, url, source, first_seen, score, state, signals)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'seen', ?10)
           ON CONFLICT(chain, address) DO UPDATE SET
             name=excluded.name, deployer=excluded.deployer, pair=excluded.pair,
             url=excluded.url, source=excluded.source, score=excluded.score, signals=excluded.signals"#,
        params![chain, normalize(address), name, deployer, pair, url, source, now, score, sig],
    )?;
    conn.query_row("SELECT id FROM targets WHERE chain=?1 AND address=?2", params![chain, normalize(address)], |r| r.get(0)).map_err(Into::into)
}

pub fn list(conn: &Connection) -> Result<Vec<Target>> { list_kind(conn, None) }

pub fn list_kind(conn: &Connection, kind: Option<&str>) -> Result<Vec<Target>> {
    let mut stmt = conn.prepare("SELECT id, chain, address, name, deployer, pair, url, source, first_seen, score, state, signals FROM targets ORDER BY score DESC, id DESC")?;
    let rows = stmt.query_map([], map_row)?;
    let all: Vec<Target> = rows.filter_map(|x| x.ok()).collect();
    let Some(k) = kind else { return Ok(all) };
    let k = k.to_ascii_lowercase();
    Ok(all.into_iter().filter(|t| {
        let blob = format!("{} {}", t.source, t.signals).to_ascii_lowercase();
        match k.as_str() {
            "bounty" => blob.contains("bounty") || blob.contains("immunefi") || blob.contains("cantina") || blob.contains("code4rena"),
            "indie" => !(blob.contains("bounty") || blob.contains("immunefi") || blob.contains("cantina")),
            _ => true,
        }
    }).collect())
}

pub fn get(conn: &Connection, id: i64) -> Result<Target> {
    conn.query_row("SELECT id, chain, address, name, deployer, pair, url, source, first_seen, score, state, signals FROM targets WHERE id=?1", params![id], map_row).context("target not found")
}

pub fn set_state(conn: &Connection, id: i64, state: &str) -> Result<()> {
    let n = conn.execute("UPDATE targets SET state=?1 WHERE id=?2", params![state, id])?;
    if n == 0 { anyhow::bail!("target {id} not found"); }
    Ok(())
}

pub fn export_csv(conn: &Connection, path: &Path) -> Result<usize> {
    let rows = list(conn)?;
    let mut out = String::from("id,chain,address,name,pair,url,source,score,state,first_seen\n");
    for t in &rows {
        out.push_str(&format!("{}\n", [t.id.to_string(), csv(&t.chain), csv(&t.address), csv(&t.name), csv(&t.pair), csv(&t.url), csv(&t.source), t.score.to_string(), csv(&t.state), csv(&t.first_seen)].join(",")));
    }
    std::fs::write(path, out)?;
    Ok(rows.len())
}

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<Target> {
    Ok(Target { id: r.get(0)?, chain: r.get(1)?, address: r.get(2)?, name: r.get(3)?, deployer: r.get(4)?, pair: r.get(5)?, url: r.get(6)?, source: r.get(7)?, first_seen: r.get(8)?, score: r.get(9)?, state: r.get(10)?, signals: r.get(11)? })
}
fn normalize(addr: &str) -> String { if addr.starts_with("0x") || addr.starts_with("0X") { addr.to_lowercase() } else { addr.to_string() } }
fn csv(s: &str) -> String { if s.contains(',') || s.contains('"') { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() } }
