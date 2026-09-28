use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Found {
    pub chain: String, pub address: String, pub name: String, pub pair: String,
    pub url: String, pub source: String, pub signals: Vec<String>, pub score: i64,
}

pub struct FetchedSource {
    pub files: Vec<(String, String)>,
    pub value_fns: Vec<String>,
    pub abi_fns: Vec<String>,
    pub deployer: String,
    pub compiler: String,
    pub contract_name: String,
    pub kind: String,
    pub heat: String,
    pub skip_reason: String,
}

pub fn latest_profiles(limit: usize) -> Result<Vec<Found>> {
    let raw: serde_json::Value = ureq::get("https://api.dexscreener.com/token-profiles/latest/v1").call().context("dexscreener token-profiles")?.into_json()?;
    let mut out = Vec::new();
    for item in raw.as_array().cloned().unwrap_or_default().into_iter().take(limit) {
        let address = item.get("tokenAddress").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if address.is_empty() { continue; }
        let chain = item.get("chainId").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mut signals = vec!["dexscreener".into(), "fresh-profile".into()];
        if chain == "solana" { signals.push("solana".into()); } else { signals.push("evm".into()); }
        let score = 3 + signals.len() as i64;
        out.push(Found { name: address.chars().take(10).collect(), chain, address, pair: String::new(), url, source: "dexscreener-profile".into(), signals, score });
    }
    Ok(out)
}

pub fn search(query: &str, limit: usize) -> Result<Vec<Found>> {
    let raw: SearchResp = ureq::get(&format!("https://api.dexscreener.com/latest/dex/search?q={}", qe(query))).call().context("dexscreener search")?.into_json()?;
    let mut out = Vec::new();
    for p in raw.pairs.into_iter().take(limit) {
        let Some(base) = p.base_token else { continue };
        if base.address.is_empty() { continue; }
        let mut signals = vec!["dexscreener".into(), "search".into()];
        if p.liquidity.as_ref().and_then(|l| l.usd).unwrap_or(0.0) < 50_000.0 { signals.push("small-liq".into()); }
        let name = if base.name.is_empty() { base.symbol } else { base.name };
        out.push(Found { chain: p.chain_id, address: base.address, name, pair: p.pair_address, url: p.url, source: "dexscreener-search".into(), signals, score: 2 + signals.len() as i64 });
    }
    Ok(out)
}

pub fn sourcify_latest(chain_id: u64, limit: usize) -> Result<Vec<Found>> {
    let raw: SourcifyList = ureq::get(&format!("https://sourcify.dev/server/v2/contracts/{chain_id}?limit={limit}&sort=desc")).call().context("sourcify list")?.into_json()?;
    let chain = chain_name(chain_id);
    let mut out = Vec::new();
    for c in raw.results {
        if c.address.is_empty() { continue; }
        let mut signals = vec!["sourcify".into(), "verified".into()];
        if c.match_kind.contains("exact") { signals.push("exact-match".into()); }
        let page = format!("https://repo.sourcify.dev/{chain_id}/{}", c.address);
        out.push(Found { chain: chain.clone(), address: c.address, name: "verified".into(), pair: String::new(), url: page, source: "sourcify".into(), signals, score: 5 + signals.len() as i64 });
    }
    Ok(out)
}

pub fn sourcify_files(chain_id: u64, address: &str) -> Result<FetchedSource> {
    let raw: serde_json::Value = ureq::get(&format!("https://sourcify.dev/server/v2/contract/{chain_id}/{address}?fields=sources,abi,compilation,deployment")).call().context("sourcify contract")?.into_json()?;
    let mut files = Vec::new();
    if let Some(map) = raw.get("sources").and_then(|s| s.as_object()) {
        for (path, body) in map {
            let content = body.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
            if !content.is_empty() { files.push((path.clone(), content)); }
        }
    }
    if files.is_empty() { anyhow::bail!("no sourcify sources for {address} on chain {chain_id}"); }
    let abi_fns = abi_all_fns(raw.get("abi"));
    let value_fns = abi_value_fns(raw.get("abi"));
    let contract_name = raw.pointer("/compilation/name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let paths: Vec<String> = files.iter().map(|(p, _)| p.to_ascii_lowercase()).collect();
    let (kind, heat, skip_reason) = classify(&contract_name, &paths, &abi_fns, &value_fns);
    Ok(FetchedSource {
        files, value_fns, abi_fns, contract_name, kind, heat, skip_reason,
        deployer: raw.pointer("/deployment/deployer").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        compiler: raw.pointer("/compilation/compilerVersion").or_else(|| raw.pointer("/compilation/compiler/version")).and_then(|v| v.as_str()).unwrap_or("").to_string(),
    })
}

fn abi_all_fns(abi: Option<&serde_json::Value>) -> Vec<String> {
    let Some(arr) = abi.and_then(|v| v.as_array()) else { return Vec::new() };
    let mut out = Vec::new();
    for item in arr {
        if item.get("type").and_then(|t| t.as_str()) != Some("function") { continue; }
        if let Some(name) = item.get("name").and_then(|n| n.as_str()) { if !name.is_empty() { out.push(name.to_string()); } }
    }
    out.sort(); out.dedup(); out
}

fn classify(name: &str, paths: &[String], abi_fns: &[String], value_fns: &[String]) -> (String, String, String) {
    let n = name.to_ascii_lowercase();
    let blob = paths.join(" ");
    let oz_proxy = n.contains("proxy") || blob.contains("erc1967proxy") || blob.contains("/proxy/proxy.sol");
    let only_shell = abi_fns.is_empty() || (abi_fns.len() <= 2 && oz_proxy);
    if oz_proxy && value_fns.is_empty() {
        return ("proxy".into(), "skip".into(), "oz-proxy-only — hunt the implementation".into());
    }
    if only_shell && value_fns.is_empty() {
        return ("unknown".into(), "skip".into(), "no value-moving abi names".into());
    }
    let kind = if n.contains("vault") || value_fns.iter().any(|f| { let x = f.to_ascii_lowercase(); x.contains("harvest") || x.contains("withdraw") }) { "vault" }
        else if n.contains("token") || value_fns.iter().any(|f| { let x = f.to_ascii_lowercase(); x.contains("mint") || x.contains("burn") }) { "token" }
        else if oz_proxy { "proxy" } else { "impl" };
    let heat = if value_fns.iter().any(|f| { let x = f.to_ascii_lowercase(); x.contains("upgrade") || x.contains("initialize") }) { "hunt" }
        else if !value_fns.is_empty() { "maybe" } else { "skip" };
    (kind.into(), heat.into(), String::new())
}

fn abi_value_fns(abi: Option<&serde_json::Value>) -> Vec<String> {
    let Some(arr) = abi.and_then(|v| v.as_array()) else { return Vec::new() };
    let keys = ["withdraw","harvest","mint","burn","execute","upgrade","initialize","init","owner","sweep","claim","borrow","liquidate","redeem","deposit","stake"];
    let mut out = Vec::new();
    for item in arr {
        if item.get("type").and_then(|t| t.as_str()) != Some("function") { continue; }
        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if keys.iter().any(|k| name.to_ascii_lowercase().contains(k)) { out.push(name.to_string()); }
    }
    out.sort(); out.dedup(); out
}

pub fn chain_id_of(name: &str) -> u64 {
    match name.to_ascii_lowercase().as_str() {
        "eth"|"ethereum"|"mainnet" => 1, "base" => 8453, "arb"|"arbitrum" => 42161,
        "op"|"optimism" => 10, "bsc"|"bnb" => 56, "polygon"|"matic" => 137,
        "avax"|"avalanche" => 43114, other => other.parse().unwrap_or(8453),
    }
}
fn chain_name(id: u64) -> String {
    match id { 1=>"ethereum".into(),8453=>"base".into(),42161=>"arbitrum".into(),10=>"optimism".into(),56=>"bsc".into(),137=>"polygon".into(),43114=>"avalanche".into(), n=>n.to_string() }
}
#[derive(Deserialize)] struct SearchResp { #[serde(default)] pairs: Vec<Pair> }
#[derive(Deserialize)] struct Pair {
    #[serde(default, rename="chainId")] chain_id: String,
    #[serde(default)] url: String,
    #[serde(default, rename="pairAddress")] pair_address: String,
    #[serde(default, rename="baseToken")] base_token: Option<Token>,
    #[serde(default)] liquidity: Option<Liq>,
}
#[derive(Deserialize)] struct Token { #[serde(default)] address: String, #[serde(default)] name: String, #[serde(default)] symbol: String }
#[derive(Deserialize)] struct Liq { #[serde(default)] usd: Option<f64> }
#[derive(Deserialize)] struct SourcifyList { #[serde(default)] results: Vec<SourcifyContract> }
#[derive(Deserialize)] struct SourcifyContract { #[serde(default)] address: String, #[serde(default, rename="match")] match_kind: String }
fn qe(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z'|b'a'..=b'z'|b'0'..=b'9'|b'-'|b'_'|b'.'|b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
