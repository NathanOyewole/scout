use anyhow::Result;
use std::path::Path;

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

    function setUp() public {{
        // forge test --fork-url $RPC
    }}

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
    println!("poc {}", dir.display());
    println!("rpc {rpc}");
    println!();
    println!("cd {}", dir.join("poc").display());
    println!("forge install foundry-rs/forge-std --no-commit");
    println!("forge test --fork-url {rpc} -vv");
    println!();
    println!("no --broadcast. fork only.");
    Ok(())
}
