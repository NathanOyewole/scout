use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;

pub const STATES: [&str; 8] = [
    "seen",
    "scoped",
    "hypothesized",
    "proven",
    "killed",
    "reported",
    "patched",
    "closed",
];

const COLS: &str = "id, chain, address, name, deployer, pair, url, source, first_seen, score, state, signals, heat, severity, contract";

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
    pub heat: String,
    pub severity: String,
    pub contract: String,
}

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path).context("open sqlite")?;
    init(&conn)?;
    Ok(conn)
}

pub fn init(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode=WAL;
        PRAGMA foreign_keys=ON;
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
            heat TEXT NOT NULL DEFAULT '',
            severity TEXT NOT NULL DEFAULT '',
            contract TEXT NOT NULL DEFAULT '',
            UNIQUE(chain, address)
        );
    "#,
    )
    .context("create schema")?;
    migrate(conn)
}

/// Add one column at a time, only when missing. The old code ran all three
/// ALTERs in a single execute_batch and swallowed the error, so a database
/// that already had `pair` but not `url` never got `url` added and every
/// later SELECT failed.
fn migrate(conn: &Connection) -> Result<()> {
    let have: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(targets)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        let mut v = Vec::new();
        for r in rows {
            v.push(r?);
        }
        v
    };
    for col in ["pair", "url", "source", "heat", "severity", "contract"] {
        if !have.iter().any(|h| h == col) {
            conn.execute_batch(&format!(
                "ALTER TABLE targets ADD COLUMN {col} TEXT NOT NULL DEFAULT ''"
            ))
            .with_context(|| format!("migrate column {col}"))?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn upsert(
    conn: &Connection,
    chain: &str,
    address: &str,
    name: &str,
    deployer: &str,
    pair: &str,
    url: &str,
    source: &str,
    score: i64,
    signals: &[String],
) -> Result<i64> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let sig = serde_json::to_string(signals)?;
    // An empty incoming value must not wipe what we already learned, and state
    // is deliberately absent from the conflict clause so re-pulling never
    // resets hunt progress.
    conn.execute(
        r#"INSERT INTO targets (chain, address, name, deployer, pair, url, source, first_seen, score, state, signals)
           VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'seen', ?10)
           ON CONFLICT(chain, address) DO UPDATE SET
             -- name is write-once. hunt_dir is "{id}-{sanitize(name)}", so
             -- rewriting it here would move an in-flight hunt folder out from
             -- under the user and orphan every note and poc in it.
             name=CASE WHEN targets.name<>'' THEN targets.name ELSE excluded.name END,
             deployer=CASE WHEN excluded.deployer<>'' THEN excluded.deployer ELSE targets.deployer END,
             pair=CASE WHEN excluded.pair<>'' THEN excluded.pair ELSE targets.pair END,
             url=CASE WHEN excluded.url<>'' THEN excluded.url ELSE targets.url END,
             source=excluded.source, score=excluded.score, signals=excluded.signals"#,
        params![chain, normalize(address), name, deployer, pair, url, source, now, score, sig],
    )
    .context("upsert target")?;
    conn.query_row(
        "SELECT id FROM targets WHERE chain=?1 AND address=?2",
        params![chain, normalize(address)],
        |r| r.get(0),
    )
    .context("re-read upserted id")
}

pub fn list(conn: &Connection) -> Result<Vec<Target>> {
    list_kind(conn, None)
}

pub fn list_kind(conn: &Connection, kind: Option<&str>) -> Result<Vec<Target>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {COLS} FROM targets ORDER BY score DESC, id DESC"
        ))
        .context("prepare list")?;
    let rows = stmt.query_map([], map_row).context("query list")?;
    // The old code did rows.filter_map(|x| x.ok()) which turned a decode
    // failure into a silently shorter list — or an empty one.
    let mut all = Vec::new();
    for r in rows {
        all.push(r.context("decode target row")?);
    }
    let Some(k) = kind else { return Ok(all) };
    let k = k.to_ascii_lowercase();
    Ok(all
        .into_iter()
        .filter(|t| {
            let blob = format!("{} {} {}", t.source, t.signals, t.name).to_ascii_lowercase();
            match k.as_str() {
                "bounty" => [
                    "bounty",
                    "immunefi",
                    "cantina",
                    "code4rena",
                    "sherlock",
                    "immunefi-program",
                ]
                .iter()
                .any(|w| blob.contains(w)),
                "indie" => !["bounty", "immunefi", "cantina", "code4rena", "sherlock"]
                    .iter()
                    .any(|w| blob.contains(w)),
                _ => true,
            }
        })
        .collect())
}

pub fn get(conn: &Connection, id: i64) -> Result<Target> {
    conn.query_row(
        &format!("SELECT {COLS} FROM targets WHERE id=?1"),
        params![id],
        map_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => anyhow::anyhow!("target {id} not found"),
        other => anyhow::Error::new(other).context(format!("read target {id}")),
    })
}

pub fn set_state(conn: &Connection, id: i64, state: &str) -> Result<()> {
    if !STATES.contains(&state) {
        anyhow::bail!("bad state '{state}' — use one of {}", STATES.join("|"));
    }
    let n = conn
        .execute(
            "UPDATE targets SET state=?1 WHERE id=?2",
            params![state, id],
        )
        .context("set state")?;
    if n == 0 {
        anyhow::bail!("target {id} not found");
    }
    Ok(())
}

/// Whitelisted so a column name can never be caller-controlled SQL.
pub fn set_field(conn: &Connection, id: i64, column: &str, value: &str) -> Result<()> {
    if ![
        "heat", "severity", "contract", "deployer", "url", "name", "signals",
    ]
    .contains(&column)
    {
        anyhow::bail!("refusing to write column {column}");
    }
    let n = conn
        .execute(
            &format!("UPDATE targets SET {column}=?1 WHERE id=?2"),
            params![value, id],
        )
        .with_context(|| format!("set {column}"))?;
    if n == 0 {
        anyhow::bail!("target {id} not found");
    }
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let n = conn
        .execute("DELETE FROM targets WHERE id=?1", params![id])
        .context("delete target")?;
    if n == 0 {
        anyhow::bail!("target {id} not found");
    }
    Ok(())
}

pub fn export_csv(conn: &Connection, path: &Path) -> Result<usize> {
    let rows = list(conn)?;
    let mut out = String::from("id,chain,address,name,contract,pair,url,source,score,state,heat,severity,signals,first_seen\n");
    for t in &rows {
        let cells = [
            t.id.to_string(),
            csv(&t.chain),
            csv(&t.address),
            csv(&t.name),
            csv(&t.contract),
            csv(&t.pair),
            csv(&t.url),
            csv(&t.source),
            t.score.to_string(),
            csv(&t.state),
            csv(&t.heat),
            csv(&t.severity),
            csv(&t.signals),
            csv(&t.first_seen),
        ];
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p)?;
    }
    // Write then rename: a crash mid-write used to leave a truncated cas.csv,
    // which is the file you actually paste into a program submission.
    let tmp = path.with_extension("csv.tmp");
    std::fs::write(&tmp, &out).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("publish {}", path.display()))?;
    Ok(rows.len())
}

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<Target> {
    Ok(Target {
        id: r.get(0)?,
        chain: r.get(1)?,
        address: r.get(2)?,
        name: r.get(3)?,
        deployer: r.get(4)?,
        pair: r.get(5)?,
        url: r.get(6)?,
        source: r.get(7)?,
        first_seen: r.get(8)?,
        score: r.get(9)?,
        state: r.get(10)?,
        signals: r.get(11)?,
        heat: r.get(12)?,
        severity: r.get(13)?,
        contract: r.get(14)?,
    })
}

fn normalize(addr: &str) -> String {
    let a = addr.trim();
    if a.starts_with("0x") || a.starts_with("0X") {
        a.to_ascii_lowercase()
    } else {
        a.to_string()
    }
}

fn csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem() -> Connection {
        // In-memory: the tests share a process, and a shared file made them
        // race each other's schema changes.
        let c = Connection::open_in_memory().unwrap();
        init(&c).unwrap();
        c
    }

    #[test]
    fn normalize_lowercases_only_hex_addresses() {
        assert_eq!(normalize("0xAbCd"), "0xabcd");
        assert_eq!(normalize("  0xAbCd  "), "0xabcd");
        assert_eq!(
            normalize("So11111111111111111111111111111111111111112"),
            "So11111111111111111111111111111111111111112"
        );
    }

    #[test]
    fn upsert_is_idempotent_per_chain_address() {
        let c = mem();
        let a = upsert(
            &c,
            "base",
            "0xAA",
            "one",
            "",
            "",
            "",
            "manual",
            4,
            &["a".into()],
        )
        .unwrap();
        let b = upsert(
            &c,
            "base",
            "0xaa",
            "one-renamed",
            "",
            "",
            "",
            "sourcify",
            9,
            &["b".into()],
        )
        .unwrap();
        assert_eq!(a, b, "same address must reuse the row id");
        let t = get(&c, a).unwrap();
        // Name is write-once: hunt_dir is derived from it, so renaming would
        // orphan the folder. See reupsert_never_renames_a_target.
        assert_eq!(
            t.name, "one",
            "first name wins so the hunt folder stays put"
        );
        assert_eq!(t.score, 9, "score is refreshed");
    }

    #[test]
    fn upsert_keeps_state_and_learned_fields() {
        let c = mem();
        let id = upsert(
            &c,
            "base",
            "0xbb",
            "n",
            "",
            "",
            "https://x",
            "manual",
            4,
            &[],
        )
        .unwrap();
        set_state(&c, id, "hypothesized").unwrap();
        set_field(&c, id, "heat", "hunt").unwrap();
        upsert(&c, "base", "0xbb", "n", "", "", "", "sourcify", 5, &[]).unwrap();
        let t = get(&c, id).unwrap();
        assert_eq!(
            t.state, "hypothesized",
            "re-pull must not reset hunt progress"
        );
        assert_eq!(t.heat, "hunt");
        assert_eq!(
            t.url, "https://x",
            "empty url must not wipe the explorer link"
        );
    }

    #[test]
    fn set_state_rejects_unknown_state() {
        let c = mem();
        let id = upsert(&c, "base", "0xcc", "n", "", "", "", "manual", 1, &[]).unwrap();
        assert!(set_state(&c, id, "vulnerable").is_err());
    }

    #[test]
    fn set_field_rejects_unknown_column() {
        let c = mem();
        let id = upsert(&c, "base", "0xdd", "n", "", "", "", "manual", 1, &[]).unwrap();
        assert!(set_field(&c, id, "state", "seen").is_err());
        assert!(set_field(&c, id, "id", "1").is_err());
        assert!(set_field(&c, id, "score", "9").is_err());
        // The columns scout actually writes must be allowed.
        for col in [
            "heat", "severity", "contract", "deployer", "url", "name", "signals",
        ] {
            assert!(
                set_field(&c, id, col, "v").is_ok(),
                "{col} must be writable"
            );
        }
    }

    #[test]
    fn every_writable_column_exists_in_the_schema() {
        // Guards the whitelist against a column that migrate forgot to add.
        let c = mem();
        let id = upsert(&c, "base", "0xee", "n", "", "", "", "manual", 1, &[]).unwrap();
        for col in [
            "heat", "severity", "contract", "deployer", "url", "name", "signals",
        ] {
            set_field(&c, id, col, "v").unwrap_or_else(|e| panic!("{col}: {e}"));
        }
    }

    #[test]
    fn set_state_on_missing_row_errors() {
        let c = mem();
        assert!(set_state(&c, 4242, "seen").is_err());
    }

    #[test]
    fn kind_filter_splits_bounty_and_indie() {
        let c = mem();
        upsert(
            &c,
            "base",
            "0x01",
            "a",
            "",
            "",
            "",
            "sourcify",
            7,
            &["indie".into()],
        )
        .unwrap();
        upsert(
            &c,
            "base",
            "0x02",
            "b",
            "",
            "",
            "",
            "bounty:immunefi",
            7,
            &["bounty".into(), "immunefi".into()],
        )
        .unwrap();
        assert_eq!(list_kind(&c, Some("bounty")).unwrap().len(), 1);
        assert_eq!(list_kind(&c, Some("indie")).unwrap().len(), 1);
        assert_eq!(list_kind(&c, None).unwrap().len(), 2);
    }

    #[test]
    fn migrate_adds_missing_columns_to_legacy_db() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch(
            "CREATE TABLE targets (id INTEGER PRIMARY KEY AUTOINCREMENT, chain TEXT NOT NULL,
             address TEXT NOT NULL, name TEXT NOT NULL DEFAULT '', deployer TEXT NOT NULL DEFAULT '',
             first_seen TEXT NOT NULL, score INTEGER NOT NULL DEFAULT 0, state TEXT NOT NULL DEFAULT 'seen',
             signals TEXT NOT NULL DEFAULT '[]', UNIQUE(chain,address));
             INSERT INTO targets (chain,address,first_seen) VALUES ('base','0x01','2026-01-01');",
        )
        .unwrap();
        migrate(&c).unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM targets WHERE heat='' AND severity='' AND contract='' AND url='' AND pair=''", [], |r| r.get(0)).unwrap();
        assert_eq!(
            n, 1,
            "legacy row must get every new column defaulting to empty"
        );
    }

    #[test]
    fn reupsert_never_renames_a_target() {
        // hunt_dir is "{id}-{sanitize(name)}". If upsert rewrote `name`, a
        // re-pull from the radar would silently move the hunt folder and
        // orphan every note and poc in it.
        let c = mem();
        let id = upsert(
            &c,
            "base",
            "0xa1",
            "Original",
            "",
            "",
            "",
            "sourcify",
            5,
            &["one".into()],
        )
        .unwrap();
        let dir_before = format!("{id}-Original");
        upsert(
            &c,
            "base",
            "0xa1",
            "Renamed",
            "",
            "",
            "",
            "dexscreener",
            9,
            &["two".into()],
        )
        .unwrap();
        let t = get(&c, id).unwrap();
        assert_eq!(t.name, "Original", "name is write-once");
        assert!(dir_before.contains(&t.name));
        // …but fresher facts and tags must still land.
        assert_eq!(t.signals, r#"["two"]"#);
        assert_eq!(t.score, 9);
        assert_eq!(t.source, "dexscreener");
    }

    #[test]
    fn upsert_adopts_a_name_when_the_row_had_none() {
        let c = mem();
        let id = upsert(&c, "base", "0xb1", "", "", "", "", "sourcify", 1, &[]).unwrap();
        upsert(
            &c,
            "base",
            "0xb1",
            "LateName",
            "",
            "",
            "",
            "sourcify",
            1,
            &[],
        )
        .unwrap();
        assert_eq!(get(&c, id).unwrap().name, "LateName");
    }

    #[test]
    fn delete_removes_exactly_one_row() {
        let c = mem();
        let a = upsert(&c, "base", "0xa1", "a", "", "", "", "manual", 1, &[]).unwrap();
        let b = upsert(&c, "base", "0xb1", "b", "", "", "", "manual", 1, &[]).unwrap();
        delete(&c, a).unwrap();
        assert!(get(&c, a).is_err());
        assert!(get(&c, b).is_ok());
        assert!(delete(&c, a).is_err());
    }

    #[test]
    fn export_csv_is_complete_and_escapes() {
        let c = mem();
        upsert(&c, "base", "0x01", "a, b", "", "", "", "manual", 1, &[]).unwrap();
        upsert(&c, "base", "0x02", "c", "", "", "", "manual", 1, &[]).unwrap();
        let p = std::env::temp_dir().join("scout_db_test.csv");
        assert_eq!(export_csv(&c, &p).unwrap(), 2);
        let body = std::fs::read_to_string(&p).unwrap();
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 3, "header + 2 rows, got {body}");
        assert!(body.contains("\"a, b\""), "commas must be quoted: {body}");
        assert!(
            !Path::new(&p).with_extension("csv.tmp").exists(),
            "temp file must be renamed away"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn list_surfaces_decode_errors_instead_of_hiding_them() {
        let c = mem();
        c.execute_batch("ALTER TABLE targets DROP COLUMN heat")
            .unwrap();
        assert!(
            list(&c).is_err(),
            "a missing column must be an error, not an empty list"
        );
    }

    #[test]
    fn open_twice_is_idempotent() {
        let p = std::env::temp_dir().join("scout_open_twice.db");
        let _ = std::fs::remove_file(&p);
        drop(open(&p).unwrap());
        let c = open(&p).unwrap();
        let id = upsert(&c, "base", "0x01", "n", "", "", "", "manual", 1, &[]).unwrap();
        assert_eq!(get(&c, id).unwrap().address, "0x01");
        drop(c);
        // Re-opening an existing database must not try to re-add columns.
        let c2 = open(&p).unwrap();
        assert_eq!(list(&c2).unwrap().len(), 1);
        let _ = std::fs::remove_file(&p);
    }
}
