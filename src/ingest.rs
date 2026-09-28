use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Found {
    pub chain: String,
    pub address: String,
    pub name: String,
    pub pair: String,
    pub url: String,
    pub source: String,
    pub signals: Vec<String>,
    pub score: i64,
}

pub fn latest_profiles(limit: usize) -> Result<Vec<Found>> {
    let raw: serde_json::Value = ureq::get("https://api.dexscreener.com/token-profiles/latest/v1")
        .call()
        .context("dexscreener token-profiles")?
        .into_json()
        .context("parse token-profiles")?;

    let arr = raw.as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for item in arr.into_iter().take(limit) {
        let chain = item
            .get("chainId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let address = item
            .get("tokenAddress")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if address.is_empty() {
            continue;
        }
        let url = item
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let desc = item
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let name = guess_name(&desc, &address);
        let mut signals = vec!["dexscreener".into(), "fresh-profile".into()];
        if is_evm(&chain) {
            signals.push("evm".into());
        }
        if chain == "solana" {
            signals.push("solana".into());
        }
        let score = 3 + signals.len() as i64;
        out.push(Found {
            chain,
            address,
            name,
            pair: String::new(),
            url,
            source: "dexscreener-profile".into(),
            signals,
            score,
        });
    }
    Ok(out)
}

pub fn search(query: &str, limit: usize) -> Result<Vec<Found>> {
    let url = format!(
        "https://api.dexscreener.com/latest/dex/search?q={}",
        urlencoding_lite(query)
    );
    let raw: SearchResp = ureq::get(&url)
        .call()
        .context("dexscreener search")?
        .into_json()
        .context("parse search")?;

    let mut out = Vec::new();
    for p in raw.pairs.into_iter().take(limit) {
        let Some(base) = p.base_token else { continue };
        if base.address.is_empty() {
            continue;
        }
        let mut signals = vec!["dexscreener".into(), "search".into()];
        if p.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0) < 50_000.0 {
            signals.push("small-liq".into());
        }
        if let Some(ts) = p.pair_created_at {
            let age_ms = chrono::Utc::now().timestamp_millis() - ts;
            if age_ms < 14 * 24 * 3600 * 1000 {
                signals.push("deployed<14d".into());
            }
        }
        let score = 2 + signals.len() as i64;
        out.push(Found {
            chain: p.chain_id,
            address: base.address,
            name: if base.name.is_empty() {
                base.symbol
            } else {
                base.name
            },
            pair: p.pair_address,
            url: p.url,
            source: "dexscreener-search".into(),
            signals,
            score,
        });
    }
    Ok(out)
}

#[derive(Deserialize)]
struct SearchResp {
    #[serde(default)]
    pairs: Vec<Pair>,
}

#[derive(Deserialize)]
struct Pair {
    #[serde(default, rename = "chainId")]
    chain_id: String,
    #[serde(default)]
    url: String,
    #[serde(default, rename = "pairAddress")]
    pair_address: String,
    #[serde(default, rename = "baseToken")]
    base_token: Option<Token>,
    #[serde(default)]
    liquidity: Option<Liq>,
    #[serde(default, rename = "pairCreatedAt")]
    pair_created_at: Option<i64>,
}

#[derive(Deserialize)]
struct Token {
    #[serde(default)]
    address: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    symbol: String,
}

#[derive(Deserialize)]
struct Liq {
    #[serde(default)]
    usd: Option<f64>,
}

fn guess_name(desc: &str, address: &str) -> String {
    let first = desc
        .split(|c: char| c == '\n' || c == '.' || c == ',')
        .next()
        .unwrap_or("")
        .trim();
    if first.is_empty() || first.len() > 40 {
        address.chars().take(8).collect()
    } else {
        first.to_string()
    }
}

fn is_evm(chain: &str) -> bool {
    matches!(
        chain,
        "ethereum" | "base" | "arbitrum" | "optimism" | "bsc" | "polygon" | "avalanche" | "blast"
    )
}

fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
