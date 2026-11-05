use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::time::Duration;

const UA: &str = concat!("scout/", env!("CARGO_PKG_VERSION"));

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
    pub max_bounty: i64,
}

#[derive(Debug, Clone)]
pub struct FnDecl {
    pub name: String,
    /// Canonical signature, e.g. `withdraw(address,uint256)`.
    pub sig: String,
    /// Full solidity declaration, e.g. `function withdraw(address,uint256) external returns (uint256)`.
    pub decl: String,
}

#[derive(Debug, Clone)]
pub struct FetchedSource {
    pub files: Vec<(String, String)>,
    pub value_fns: Vec<String>,
    pub admin_fns: Vec<String>,
    pub read_fns: Vec<String>,
    pub abi_fns: Vec<String>,
    pub decls: Vec<FnDecl>,
    pub deployer: String,
    pub compiler: String,
    pub contract_name: String,
    pub match_kind: String,
    pub kind: String,
    pub heat: String,
    pub skip_reason: String,
    pub notes: Vec<String>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(45))
        .user_agent(UA)
        .build()
}

fn get_json<T: DeserializeOwned>(what: &str, url: &str) -> Result<T> {
    let agent = agent();
    let mut wait = Duration::from_secs(2);
    for attempt in 1..=4u32 {
        match agent.get(url).call() {
            Ok(resp) => return resp.into_json().with_context(|| format!("decode {what}")),
            Err(ureq::Error::Status(code, resp)) if code == 429 || (500..600).contains(&code) => {
                if attempt == 4 {
                    anyhow::bail!("{what}: HTTP {code} after {attempt} tries — {url}");
                }
                let retry_after = resp
                    .header("retry-after")
                    .and_then(|v| v.trim().parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or(wait)
                    .min(Duration::from_secs(30));
                eprintln!(
                    "  {what}: HTTP {code}, retrying in {}s",
                    retry_after.as_secs()
                );
                std::thread::sleep(retry_after);
                wait = (wait * 2).min(Duration::from_secs(16));
            }
            Err(ureq::Error::Status(code, _)) => anyhow::bail!("{what}: HTTP {code} — {url}"),
            Err(e) => return Err(e).with_context(|| format!("{what} request failed")),
        }
    }
    anyhow::bail!("{what}: gave up — {url}")
}

pub fn latest_profiles(limit: usize) -> Result<Vec<Found>> {
    let raw: Vec<serde_json::Value> = get_json(
        "dexscreener profiles",
        "https://api.dexscreener.com/token-profiles/latest/v1",
    )?;
    let mut out = Vec::new();
    for item in raw.into_iter().take(limit) {
        let address = item
            .get("tokenAddress")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if address.is_empty() {
            continue;
        }
        let chain = item
            .get("chainId")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let url = item
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let mut signals = vec!["dexscreener".into(), "fresh-profile".into()];
        if chain_id(&chain).is_some() {
            signals.push("evm".into());
        } else {
            signals.push("non-evm".into());
        }
        let score = 3 + signals.len() as i64;
        let name = if address.starts_with("0x") {
            address.chars().take(10).collect()
        } else {
            address.chars().take(12).collect()
        };
        out.push(Found {
            name,
            chain,
            address,
            pair: String::new(),
            url,
            source: "dexscreener-profile".into(),
            signals,
            score,
            max_bounty: 0,
        });
    }
    Ok(out)
}

pub fn search(query: &str, limit: usize) -> Result<Vec<Found>> {
    let raw: SearchResp = get_json(
        "dexscreener search",
        &format!(
            "https://api.dexscreener.com/latest/dex/search?q={}",
            qe(query)
        ),
    )?;
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
        let score = 2 + signals.len() as i64;
        let name = if base.name.is_empty() {
            base.symbol
        } else {
            base.name
        };
        out.push(Found {
            chain: p.chain_id,
            address: base.address,
            name,
            pair: p.pair_address,
            url: p.url,
            source: "dexscreener-search".into(),
            signals,
            score,
            max_bounty: 0,
        });
    }
    Ok(out)
}

pub fn sourcify_latest(chain_id: u64, limit: usize) -> Result<Vec<Found>> {
    let raw: SourcifyList = get_json(
        "sourcify list",
        &format!("https://sourcify.dev/server/v2/contracts/{chain_id}?limit={limit}&sort=desc"),
    )?;
    let chain = chain_name(chain_id);
    let mut out = Vec::new();
    for c in raw.results {
        if c.address.is_empty() {
            continue;
        }
        let mut signals = vec!["sourcify".into(), "verified".into()];
        if c.match_kind == "exact_match" {
            signals.push("exact-match".into());
        } else {
            signals.push("partial-match".into());
        }
        let score = 5 + signals.len() as i64;
        let page = format!("https://repo.sourcify.dev/{chain_id}/{}", c.address);
        out.push(Found {
            chain: chain.clone(),
            address: c.address,
            name: "verified".into(),
            pair: String::new(),
            url: page,
            source: "sourcify".into(),
            signals,
            score,
            max_bounty: 0,
        });
    }
    Ok(out)
}

pub fn sourcify_files(chain_id: u64, address: &str) -> Result<FetchedSource> {
    let raw: serde_json::Value = get_json(
        "sourcify contract",
        &format!("https://sourcify.dev/server/v2/contract/{chain_id}/{address}?fields=sources,abi,compilation,deployment"),
    )?;
    let mut files = Vec::new();
    if let Some(map) = raw.get("sources").and_then(|s| s.as_object()) {
        for (path, body) in map {
            let content = body
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            if !content.is_empty() {
                files.push((path.clone(), content));
            }
        }
    }
    let match_kind = raw
        .get("match")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if files.is_empty() {
        anyhow::bail!(
            "no verified sources for {address} on chain {chain_id} (match='{match_kind}') — \
             check the CA on the explorer, or pull it with --address and read the explorer source"
        );
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let abi = raw.get("abi");
    let abi_fns = abi_fn_names(abi, None);
    let read_fns = abi_fn_names(abi, Some(true));
    let mutating = abi_fn_names(abi, Some(false));
    let (value_fns, admin_fns) = split_surface(&mutating);
    let decls = abi_decls(abi, &mutating);
    let contract_name = raw
        .pointer("/compilation/name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let paths: Vec<String> = files.iter().map(|(p, _)| p.to_ascii_lowercase()).collect();
    let (kind, heat, skip_reason, notes) = classify(
        &contract_name,
        &paths,
        &abi_fns,
        &mutating,
        &value_fns,
        &admin_fns,
        &match_kind,
    );
    Ok(FetchedSource {
        value_fns,
        admin_fns,
        read_fns,
        abi_fns,
        decls,
        contract_name,
        match_kind,
        kind,
        heat,
        skip_reason,
        notes,
        files,
        deployer: raw
            .pointer("/deployment/deployer")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        compiler: raw
            .pointer("/compilation/compilerVersion")
            .or_else(|| raw.pointer("/compilation/compiler/version"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

const MONEY_KEYS: [&str; 30] = [
    "withdraw",
    "deposit",
    "harvest",
    "mint",
    "burn",
    "redeem",
    "stake",
    "unstake",
    "claim",
    "borrow",
    "liquidate",
    "repay",
    "execute",
    "exec",
    "multicall",
    "transfer",
    "approve",
    "permit",
    "sweep",
    "rescue",
    "drain",
    "unwrap",
    "refund",
    "distribute",
    "compound",
    "swap",
    "wrap",
    "bridge",
    "release",
    "setapproval",
];

/// `want_view` reads as: None = every function, Some(true) = view/pure only,
/// Some(false) = state-changing only. (An inverted bool here silently swaps the
/// value plane for the read plane, which is how `owner()` ends up listed as a
/// money-moving function.)
fn abi_fn_names(abi: Option<&serde_json::Value>, want_view: Option<bool>) -> Vec<String> {
    let Some(arr) = abi.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        if item.get("type").and_then(|t| t.as_str()) != Some("function") {
            continue;
        }
        if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
            if name.is_empty() {
                continue;
            }
            if let Some(want) = want_view {
                let is_view = matches!(
                    item.get("stateMutability").and_then(|m| m.as_str()),
                    Some("view") | Some("pure")
                );
                if is_view != want {
                    continue;
                }
            }
            out.push(name.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Split the state-changing surface in two.
///
/// value = the name says money moves.
/// admin = everything else that still writes state. A vault whose only setter
///   is `setFeeRecipient` has no owner() in the abi, so keyword matching alone
///   would file it as a plain maybe — but that setter is exactly the knob to
///   read. Control plane plus value plane in one contract is the hot shape.
fn split_surface(mutating: &[String]) -> (Vec<String>, Vec<String>) {
    let mut value = Vec::new();
    let mut admin = Vec::new();
    for f in mutating {
        if MONEY_KEYS
            .iter()
            .any(|k| f.to_ascii_lowercase().contains(k))
        {
            value.push(f.clone());
        } else {
            admin.push(f.clone());
        }
    }
    (value, admin)
}

/// ABI types, as they may be spelled in a Solidity interface declaration.
///
/// Reference types need an explicit data location on `external` functions, and
/// solc rejects `function f(bytes memory)` with "Data location must be
/// "memory" or "calldata" ... but none was given". That is why the generated
/// TargetCalls.sol has to decorate them. `memory` is used for both inputs and
/// outputs: it is the only location valid on both sides, and it is what
/// forge-std's low-level calls and `abi.encodeCall` accept.
fn sol_decl_types(list: Option<&serde_json::Value>) -> Option<Vec<String>> {
    let arr = list?.as_array()?;
    let mut out = Vec::new();
    for i in arr {
        let t = i.get("type")?.as_str()?;
        // Tuples and arrays of tuples have no flat form here; skip the function
        // rather than emit solidity that will not compile.
        if t.starts_with("tuple") || t.contains('(') {
            return None;
        }
        out.push(with_data_location(t, "memory"));
    }
    Some(out)
}

/// solc's own test for "needs a data location": a value type is anything that
/// is not a dynamically-sized reference. Arrays of value types are reference
/// types too and need `memory` as well.
fn needs_data_location(t: &str) -> bool {
    let base = t.split('[').next().unwrap_or(t);
    matches!(base, "string" | "bytes" | "T" | "tuple") || base.ends_with(']') || t.contains('[')
}

fn with_data_location(t: &str, loc: &str) -> String {
    if needs_data_location(t) {
        format!("{t} {loc}")
    } else {
        t.to_string()
    }
}

/// Canonical signature form (`withdraw(uint256)`) for the ABI encoder. This
/// must NOT include data locations — `abi.encodeCall` and the selector depend
/// on the bare type, and `f(bytes memory)` hashes differently from
/// `f(bytes)`.
fn sol_sig_types(list: Option<&serde_json::Value>) -> Option<Vec<String>> {
    let arr = list?.as_array()?;
    let mut out = Vec::new();
    for i in arr {
        let t = i.get("type")?.as_str()?;
        if t.starts_with("tuple") || t.contains('(') {
            return None;
        }
        out.push(t.to_string());
    }
    Some(out)
}

/// State-changing declarations, so a fork test can call the target through a
/// typed interface instead of hand-writing abi.encodeWithSignature.
fn abi_decls(abi: Option<&serde_json::Value>, mutating: &[String]) -> Vec<FnDecl> {
    let Some(arr) = abi.and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in arr {
        if item.get("type").and_then(|t| t.as_str()) != Some("function") {
            continue;
        }
        let Some(name) = item.get("name").and_then(|n| n.as_str()) else {
            continue;
        };
        if !mutating.iter().any(|m| m == name) {
            continue;
        }
        // Decl uses data locations; the signature must not (it feeds the
        // selector, and `f(bytes memory)` hashes differently from `f(bytes)`).
        let Some(decl_args) = sol_decl_types(item.get("inputs")) else {
            continue;
        };
        let Some(sig_args) = sol_sig_types(item.get("inputs")) else {
            continue;
        };
        let outs = sol_decl_types(item.get("outputs"));
        let sig = format!("{name}({})", sig_args.join(","));
        let mut decl = format!("function {name}({}) external", decl_args.join(","));
        if item.get("stateMutability").and_then(|m| m.as_str()) == Some("payable") {
            decl.push_str(" payable");
        }
        if let Some(o) = outs {
            if !o.is_empty() {
                decl.push_str(&format!(" returns ({})", o.join(",")));
            }
        }
        decl.push(';');
        out.push(FnDecl {
            name: name.to_string(),
            sig,
            decl,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.sig.cmp(&b.sig)));
    out.dedup_by(|a, b| a.sig == b.sig);
    out
}

#[allow(clippy::too_many_arguments)]
fn classify(
    name: &str,
    paths: &[String],
    abi_fns: &[String],
    mutating: &[String],
    value_fns: &[String],
    admin_fns: &[String],
    match_kind: &str,
) -> (String, String, String, Vec<String>) {
    let n = name.to_ascii_lowercase();
    let blob = paths.join(" ");
    let mut notes = Vec::new();

    if match_kind != "exact_match" {
        notes.push(format!(
            "sourcify match='{match_kind}' — the source may not be the bytecode on chain. \
             verify before you trust any line of it."
        ));
    }

    let proxy_name = n.contains("proxy") || n.contains("beacon");
    let uups = mutating.iter().any(|f| {
        let x = f.to_ascii_lowercase();
        x.contains("upgradeto")
    });

    // A real proxy shell exposes at most a couple of functions and nothing that
    // moves value. The old code only skipped proxies whose abi was completely
    // empty, so a UUPS proxy shell that also exposed upgradeTo got heat=hunt.
    if abi_fns.len() <= 2
        && (proxy_name
            || blob.contains("erc1967proxy")
            || blob.contains("transparentproxy")
            || blob.contains("beaconproxy"))
    {
        return (
            "proxy".into(),
            "skip".into(),
            "proxy shell — hunt the implementation, not this address".into(),
            notes,
        );
    }

    if mutating.is_empty() {
        return (
            if proxy_name {
                "proxy".into()
            } else {
                "unknown".into()
            },
            "skip".into(),
            "no state-changing abi entry — nothing a stranger can push".into(),
            notes,
        );
    }

    let kind = if n.contains("vault")
        || value_fns.iter().any(|f| {
            let x = f.to_ascii_lowercase();
            x.contains("harvest") || x.contains("withdraw")
        }) {
        "vault"
    } else if n.contains("token")
        || value_fns.iter().any(|f| {
            let x = f.to_ascii_lowercase();
            x.contains("mint") || x.contains("burn")
        })
    {
        "token"
    } else if uups {
        "upgradeable"
    } else {
        "impl"
    };

    // Access control plus value movement in the same contract is where the
    // money bugs actually live, so that combination is the hot one.
    let mut heat = if !admin_fns.is_empty() {
        "hunt"
    } else {
        "maybe"
    };
    if uups {
        notes.push("uups: the implementation is swappable. read _authorizeUpgrade / _checkOwner, not just the proxy.".into());
    }
    if !admin_fns.is_empty() && !value_fns.is_empty() {
        notes.push(
            "control plane and value plane in one contract — the first thing to read.".into(),
        );
    }
    if heat == "hunt" && !match_kind.is_empty() && match_kind != "exact_match" {
        heat = "maybe";
        notes.push("downgraded hunt -> maybe: partial match, confirm the source first.".into());
    }
    (kind.into(), heat.into(), String::new(), notes)
}

pub fn immunefi_latest(limit: usize) -> Result<Vec<Found>> {
    let raw: serde_json::Value = get_json(
        "immunefi bounties",
        "https://immunefi.com/public-api/bounties.json",
    )?;
    let arr = raw.as_array().cloned().unwrap_or_default();
    let mut out = Vec::new();
    for b in arr.into_iter().take(limit * 4) {
        let slug = b
            .get("slug")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if slug.is_empty() {
            continue;
        }
        let name = b
            .get("project")
            .and_then(|v| v.as_str())
            .unwrap_or(&slug)
            .to_string();
        let ecosystems: Vec<String> = b
            .get("ecosystem")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        // pick first EVM chain scout understands
        let mut chain = String::new();
        for e in &ecosystems {
            let low = e.to_ascii_lowercase();
            let mapped = match low.as_str() {
                "eth" | "ethereum" => "ethereum",
                "base" => "base",
                "arb" | "arbitrum" => "arbitrum",
                "op" | "optimism" => "optimism",
                "bsc" | "bnb" | "binance" => "bsc",
                "polygon" | "matic" => "polygon",
                "avax" | "avalanche" => "avalanche",
                _ => "",
            };
            if !mapped.is_empty() {
                chain = mapped.to_string();
                break;
            }
        }
        if chain.is_empty() {
            continue;
        }
        let max_bounty = b.get("maxBounty").and_then(|v| v.as_i64()).unwrap_or(0);
        let score = (max_bounty / 10000).max(1).min(100);
        let url = format!("https://immunefi.com/bounty/{}", slug);
        let mut signals = vec![
            "immunefi".into(),
            "bounty".into(),
            "immunefi-program".into(),
        ];
        for e in &ecosystems {
            signals.push(e.to_ascii_lowercase());
        }
        if max_bounty >= 100_000 {
            signals.push("high-bounty".into());
        }
        out.push(Found {
            chain,
            // synthetic address: programs have no single CA. the user adds the
            // specific contract with `--address` once they pick one from scope.
            address: format!("program:{}", slug),
            name,
            pair: String::new(),
            url,
            source: "immunefi".into(),
            signals,
            score,
            max_bounty,
        });
        if out.len() >= limit {
            break;
        }
    }
    Ok(out)
}
pub fn chain_id(name: &str) -> Option<u64> {
    let n = name.trim().to_ascii_lowercase();
    let id = match n.as_str() {
        "base" => 8453,
        "arb" | "arbitrum" | "arbitrum-one" => 42161,
        "op" | "optimism" => 10,
        "bsc" | "bnb" | "binance" => 56,
        "polygon" | "matic" => 137,
        "avax" | "avalanche" => 43114,
        "linea" => 59144,
        "scroll" => 534352,
        "zksync" | "era" => 324,
        "celo" => 42220,
        "mantle" => 5000,
        "blast" => 81457,
        other => {
            if let Ok(id) = other.parse::<u64>() {
                id
            } else {
                return None;
            }
        }
    };
    Some(id)
}

/// Legacy behaviour: silently assumed base. That is how a solana mint from
/// dexscreener ended up being looked up as a base contract. Kept only so the
/// regression stays pinned by a test; nothing in the CLI calls it.
#[cfg(test)]
pub fn chain_id_of(name: &str) -> u64 {
    chain_id(name).unwrap_or(8453)
}

pub fn chain_name(id: u64) -> String {
    match id {
        1 => "ethereum",
        8453 => "base",
        42161 => "arbitrum",
        10 => "optimism",
        56 => "bsc",
        137 => "polygon",
        43114 => "avalanche",
        59144 => "linea",
        534352 => "scroll",
        324 => "zksync",
        n => {
            let _ = n;
            ""
        }
    }
    .to_string()
}

pub fn explorer_url(chain: &str, address: &str) -> String {
    let host = match chain.trim().to_ascii_lowercase().as_str() {
        "ethereum" | "eth" | "mainnet" => "https://etherscan.io",
        "base" => "https://basescan.org",
        "arbitrum" | "arb" => "https://arbiscan.io",
        "optimism" | "op" => "https://optimistic.etherscan.io",
        "bsc" | "bnb" => "https://bscscan.com",
        "polygon" | "matic" => "https://polygonscan.com",
        "avalanche" | "avax" => "https://snowtrace.io",
        "linea" => "https://lineascan.build",
        "scroll" => "https://scrollscan.com",
        _ => "https://sourcify.dev",
    };
    if host.contains("sourcify") {
        match chain_id(chain) {
            Some(id) => format!("{host}/server/contract/{id}/{address}"),
            None => String::new(),
        }
    } else {
        format!("{host}/address/{address}")
    }
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
#[derive(Deserialize)]
struct SourcifyList {
    #[serde(default)]
    results: Vec<SourcifyContract>,
}
#[derive(Deserialize)]
struct SourcifyContract {
    #[serde(default)]
    address: String,
    #[serde(default, rename = "match")]
    match_kind: String,
}

fn qe(s: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn abi(json: &str) -> serde_json::Value {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn non_evm_chains_are_rejected_not_silently_become_base() {
        for c in ["solana", "near", "btc", "sui", "robinhood", "Solana"] {
            assert_eq!(chain_id(c), None, "{c} must not resolve to an evm id");
        }
        assert_eq!(chain_id("base"), Some(8453));
        assert_eq!(chain_id("BASE"), Some(8453));
        assert_eq!(chain_id("11155111"), Some(11155111));
        assert_eq!(
            chain_id_of("solana"),
            8453,
            "legacy fallback still 8453 for compat"
        );
    }

    #[test]
    fn view_getters_are_not_counted_as_value_surface() {
        // The real regression: UPGRADE_INTERFACE_VERSION / owner() are view
        // entries and were being listed as value-moving functions.
        let a = abi(r#"[
            {"type":"function","name":"UPGRADE_INTERFACE_VERSION","stateMutability":"view"},
            {"type":"function","name":"owner","stateMutability":"view"},
            {"type":"function","name":"ownerAgentIndex","stateMutability":"view"},
            {"type":"function","name":"withdrawEthToUser","stateMutability":"nonpayable"},
            {"type":"function","name":"initialize","stateMutability":"nonpayable"},
            {"type":"constructor","stateMutability":"nonpayable"},
            {"type":"event","name":"Upgraded"}
        ]"#);
        assert_eq!(
            abi_fn_names(Some(&a), Some(true)).len(),
            3,
            "three view getters"
        );
        assert_eq!(
            abi_fn_names(Some(&a), None).len(),
            5,
            "events/constructors are not functions"
        );
        let mutating = abi_fn_names(Some(&a), Some(false));
        assert_eq!(
            mutating,
            vec!["initialize".to_string(), "withdrawEthToUser".to_string()]
        );
        let (value, admin) = split_surface(&mutating);
        assert_eq!(value, vec!["withdrawEthToUser".to_string()]);
        assert_eq!(admin, vec!["initialize".to_string()]);
    }

    #[test]
    fn plain_erc20_stays_a_maybe_not_a_hunt() {
        let a = abi(r#"[
            {"type":"function","name":"transfer","stateMutability":"nonpayable"},
            {"type":"function","name":"approve","stateMutability":"nonpayable"},
            {"type":"function","name":"transferFrom","stateMutability":"nonpayable"},
            {"type":"function","name":"mint","stateMutability":"nonpayable"},
            {"type":"function","name":"totalSupply","stateMutability":"view"}
        ]"#);
        let abi_fns = abi_fn_names(Some(&a), None);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let (value, admin) = split_surface(&mutating);
        assert_eq!(admin.len(), 0, "every mutating entry is a money mover here");
        let (kind, heat, _, _) = classify(
            "MyToken",
            &[],
            &abi_fns,
            &mutating,
            &value,
            &admin,
            "exact_match",
        );
        assert_eq!(kind, "token");
        assert_eq!(heat, "maybe");
    }

    #[test]
    fn uups_proxy_shell_is_skipped_even_with_upgrade_to() {
        // A BeaconProxy/ERC1967 shell with an upgradeTo entry must still be skip:
        // the address to hunt is the implementation.
        let a = abi(r#"[
            {"type":"function","name":"upgradeTo","stateMutability":"nonpayable"},
            {"type":"function","name":"admin","stateMutability":"view"}
        ]"#);
        let abi_fns = abi_fn_names(Some(&a), None);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let (value, admin) = split_surface(&mutating);
        let (kind, heat, reason, _) = classify(
            "ERC1967Proxy",
            &["contracts/proxy/erc1967/erc1967proxy.sol".into()],
            &abi_fns,
            &mutating,
            &value,
            &admin,
            "exact_match",
        );
        assert_eq!(kind, "proxy");
        assert_eq!(heat, "skip");
        assert!(reason.contains("implementation"), "got {reason}");
    }

    #[test]
    fn vault_with_control_plane_is_hot() {
        let a = abi(r#"[
            {"type":"function","name":"deposit","stateMutability":"payable"},
            {"type":"function","name":"withdraw","stateMutability":"nonpayable"},
            {"type":"function","name":"setFeeRecipient","stateMutability":"nonpayable"},
            {"type":"function","name":"totalAssets","stateMutability":"view"}
        ]"#);
        let abi_fns = abi_fn_names(Some(&a), None);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let (value, admin) = split_surface(&mutating);
        let (kind, heat, _, notes) = classify(
            "StrategyVault",
            &[],
            &abi_fns,
            &mutating,
            &value,
            &admin,
            "exact_match",
        );
        assert_eq!(kind, "vault");
        assert_eq!(heat, "hunt");
        assert!(notes.iter().any(|n| n.contains("control plane")));
    }

    #[test]
    fn partial_match_downgrades_a_hot_target() {
        let a = abi(r#"[
            {"type":"function","name":"withdraw","stateMutability":"nonpayable"},
            {"type":"function","name":"setOwner","stateMutability":"nonpayable"}
        ]"#);
        let abi_fns = abi_fn_names(Some(&a), None);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let (value, admin) = split_surface(&mutating);
        let (_, heat, _, notes) =
            classify("Vault", &[], &abi_fns, &mutating, &value, &admin, "match");
        assert_eq!(heat, "maybe");
        assert!(notes.iter().any(|n| n.contains("downgraded")));
    }

    #[test]
    fn read_only_contract_is_skipped() {
        let a = abi(r#"[{"type":"function","name":"quote","stateMutability":"view"}]"#);
        let abi_fns = abi_fn_names(Some(&a), None);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let (value, admin) = split_surface(&mutating);
        let (_, heat, reason, _) = classify(
            "Oracle",
            &[],
            &abi_fns,
            &mutating,
            &value,
            &admin,
            "exact_match",
        );
        assert_eq!(heat, "skip");
        assert!(reason.contains("state-changing"));
    }

    #[test]
    fn uups_implementation_is_flagged_as_upgradeable() {
        let a = abi(r#"[
            {"type":"function","name":"upgradeToAndCall","stateMutability":"nonpayable"},
            {"type":"function","name":"mint","stateMutability":"nonpayable"},
            {"type":"function","name":"transfer","stateMutability":"nonpayable"}
        ]"#);
        let abi_fns = abi_fn_names(Some(&a), None);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let (value, admin) = split_surface(&mutating);
        let (kind, heat, _, notes) = classify(
            "TokenV2",
            &[],
            &abi_fns,
            &mutating,
            &value,
            &admin,
            "exact_match",
        );
        assert_eq!(kind, "token", "token naming wins over upgradeable");
        assert_eq!(heat, "hunt");
        assert!(notes.iter().any(|n| n.contains("_authorizeUpgrade")));
    }

    #[test]
    fn explorer_url_is_per_chain() {
        assert_eq!(
            explorer_url("base", "0xabc"),
            "https://basescan.org/address/0xabc"
        );
        assert_eq!(
            explorer_url("ethereum", "0xabc"),
            "https://etherscan.io/address/0xabc"
        );
        assert!(explorer_url("base", "0xabc").contains("basescan.org"));
    }

    #[test]
    fn query_encoding_is_safe() {
        assert_eq!(qe("Uniswap V3"), "Uniswap+V3");
        assert_eq!(qe("a/b?c&d"), "a%2Fb%3Fc%26d");
    }

    #[test]
    fn decls_render_callable_solidity_for_mutators_only() {
        let a = abi(r#"[
            {"type":"function","name":"withdraw","stateMutability":"nonpayable",
             "inputs":[{"name":"amount","type":"uint256"}],"outputs":[{"type":"uint256"}]},
            {"type":"function","name":"pay","stateMutability":"payable","inputs":[],"outputs":[]},
            {"type":"function","name":"totalAssets","stateMutability":"view","inputs":[],"outputs":[{"type":"uint256"}]},
            {"type":"function","name":"tup","stateMutability":"nonpayable",
             "inputs":[{"type":"tuple","components":[]}],"outputs":[]}
        ]"#);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let d = abi_decls(Some(&a), &mutating);
        let names: Vec<&str> = d.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["pay", "withdraw"],
            "views and tuples are skipped"
        );
        assert_eq!(d[1].sig, "withdraw(uint256)");
        assert_eq!(
            d[1].decl,
            "function withdraw(uint256) external returns (uint256);"
        );
        assert_eq!(d[0].decl, "function pay() external payable;");
    }

    #[test]
    fn reference_types_get_a_data_location_in_declarations() {
        // solc rejects "function execute(address,uint256,bytes) external" with
        // "Data location must be memory or calldata ... none was given", which
        // is why the generated interface failed to compile.
        let a = abi(r#"[
            {"type":"function","name":"execute","stateMutability":"nonpayable",
             "inputs":[{"type":"address"},{"type":"uint256"},{"type":"bytes"}],
             "outputs":[{"type":"bytes"}]}
        ]"#);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let d = abi_decls(Some(&a), &mutating);
        assert_eq!(
            d[0].decl,
            "function execute(address,uint256,bytes memory) external returns (bytes memory);"
        );
    }

    #[test]
    fn signatures_never_carry_a_data_location() {
        // The signature feeds the selector. f(bytes memory) hashes differently
        // from f(bytes), so the sig must use the bare ABI type.
        let a = abi(r#"[
            {"type":"function","name":"submit","stateMutability":"nonpayable",
             "inputs":[{"type":"bytes"},{"type":"string"}],"outputs":[]}
        ]"#);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let d = abi_decls(Some(&a), &mutating);
        assert_eq!(d[0].sig, "submit(bytes,string)");
        assert!(d[0].decl.contains("bytes memory"), "decl is decorated");
    }

    #[test]
    fn arrays_of_value_types_also_need_memory() {
        let a = abi(r#"[
            {"type":"function","name":"batch","stateMutability":"nonpayable",
             "inputs":[{"type":"address[]"},{"type":"uint256[]"}],"outputs":[]}
        ]"#);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let d = abi_decls(Some(&a), &mutating);
        assert_eq!(
            d[0].decl,
            "function batch(address[] memory,uint256[] memory) external;"
        );
    }

    #[test]
    fn abi_decls_keeps_overloads() {
        // Solidity overloading is real and common (safeTransferFrom with and
        // without bytes). Deduping by name silently dropped the second
        // signature, so the generated interface could not call it.
        let a = abi(r#"[
            {"type":"function","name":"safeTransferFrom","stateMutability":"nonpayable",
             "inputs":[{"type":"address"},{"type":"address"},{"type":"uint256"}],"outputs":[]},
            {"type":"function","name":"safeTransferFrom","stateMutability":"nonpayable",
             "inputs":[{"type":"address"},{"type":"address"},{"type":"uint256"},{"type":"bytes"}],"outputs":[]}
        ]"#);
        let mutating = abi_fn_names(Some(&a), Some(false));
        let d = abi_decls(Some(&a), &mutating);
        assert_eq!(d.len(), 2, "both overloads must survive");
        assert!(d
            .iter()
            .any(|f| f.sig == "safeTransferFrom(address,address,uint256)"));
        assert!(d
            .iter()
            .any(|f| f.sig == "safeTransferFrom(address,address,uint256,bytes)"));
    }

    #[test]
    fn abi_decls_dedups_exact_repeats() {
        let one = r#"{"type":"function","name":"withdraw","stateMutability":"nonpayable",
             "inputs":[{"type":"uint256"}],"outputs":[]}"#;
        let a = abi(&format!("[{one},{one}]"));
        let mutating = abi_fn_names(Some(&a), Some(false));
        assert_eq!(
            abi_decls(Some(&a), &mutating).len(),
            1,
            "identical sigs collapse"
        );
    }
}
