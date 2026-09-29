use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::db::Target;

pub fn rpc_for(chain: &str) -> &'static str {
    match chain.to_ascii_lowercase().as_str() {
        "eth" | "ethereum" | "mainnet" => "https://eth.llamarpc.com",
        "base" => "https://mainnet.base.org",
        "arb" | "arbitrum" => "https://arb1.arbitrum.io/rpc",
        "op" | "optimism" => "https://mainnet.optimism.io",
        "bsc" | "bnb" => "https://bsc-dataseed.binance.org",
        "polygon" | "matic" => "https://polygon-rpc.com",
        "avax" | "avalanche" => "https://api.avax.network/ext/bc/C/rpc",
        _ => "http://127.0.0.1:8545",
    }
}

pub fn fork_test_sol(t: &Target) -> String {
    format!(
        r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
import {{Test}} from "forge-std/Test.sol";

// Local fork only. Do not broadcast.
contract HypothesisTest is Test {{
    address constant TARGET = {addr};
    function setUp() public {{}}
    function testReadTarget() public view {{
        assertTrue(TARGET.code.length > 0, "no code at CA");
    }}
    function testHypothesis() public {{
        // fill from NOTES.md
    }}
}}
"#,
        addr = t.address
    )
}

pub fn print_test_cmds(dir: &Path, t: &Target) -> Result<()> {
    let rpc = rpc_for(&t.chain);
    let poc = dir.join("poc");
    println!("poc {}", poc.display());
    println!("rpc {rpc}");
    println!();
    println!("cd {}", poc.display());
    println!("forge install foundry-rs/forge-std --no-commit");
    println!("forge test --fork-url {rpc} -vv");
    println!();
    println!("no --broadcast. fork only.");
    Ok(())
}

fn forge_bin() -> Option<String> {
    for n in ["forge", "forge.exe"] {
        if let Ok(out) = Command::new(n).arg("--version").output() {
            if out.status.success() { return Some(n.to_string()); }
        }
    }
    None
}

pub fn prove(dir: &Path, t: &Target) -> Result<i32> {
    let poc = dir.join("poc");
    let rpc = rpc_for(&t.chain);
    print_test_cmds(dir, t)?;
    let Some(forge) = forge_bin() else {
        println!();
        println!("forge not on PATH.");
        println!("Windows: get the win zip from https://github.com/foundry-rs/foundry/releases");
        println!("extract forge.exe and add that folder to Path.");
        println!("Do not curl | bash in PowerShell.");
        return Ok(127);
    };
    if !poc.join("foundry.toml").exists() {
        anyhow::bail!("no poc/foundry.toml — run scout source first");
    }
    if !poc.join("lib/forge-std").exists() {
        println!("installing forge-std...");
        let st = Command::new(&forge).current_dir(&poc).args(["install", "foundry-rs/forge-std", "--no-commit"]).status().context("forge install")?;
        if !st.success() { println!("forge install failed; run it in the poc folder"); }
    }
    println!("running fork test (no broadcast)...");
    let st = Command::new(&forge).current_dir(&poc).args(["test", "--fork-url", rpc, "-vv"]).status().context("forge test")?;
    Ok(st.code().unwrap_or(1))
}
