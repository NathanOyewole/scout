use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::db::Target;

pub const PLACEHOLDER: &str = "// fill from NOTES.md";

pub fn rpc_for(chain: &str) -> Option<String> {
    if let Ok(custom) = std::env::var("SCOUT_RPC") {
        let t = custom.trim().to_string();
        if !t.is_empty() {
            return Some(t);
        }
    }
    let r = match chain.to_ascii_lowercase().as_str() {
        "eth" | "ethereum" | "mainnet" => "https://eth.llamarpc.com",
        "base" => "https://mainnet.base.org",
        "arb" | "arbitrum" => "https://arb1.arbitrum.io/rpc",
        "op" | "optimism" => "https://mainnet.optimism.io",
        "bsc" | "bnb" => "https://bsc-dataseed.binance.org",
        "polygon" | "matic" => "https://polygon-rpc.com",
        "avax" | "avalanche" => "https://api.avax.network/ext/bc/C/rpc",
        // The old code fell through to http://127.0.0.1:8545 for anything it did
        // not know, so a typo'd chain silently forked a dead local node.
        _ => return None,
    };
    Some(r.to_string())
}

pub fn is_hex_address(addr: &str) -> bool {
    let a = addr.trim();
    a.len() == 42 && a.starts_with("0x") && a[2..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn fork_test_sol(t: &Target) -> String {
    let addr = t.address.trim().to_string();
    let rpc = rpc_for(&t.chain).unwrap_or_else(|| "<rpc for this chain>".to_string());
    let (line, usable) = if is_hex_address(&addr) {
        (format!("    address constant TARGET = {addr};"), true)
    } else {
        (
            format!(
                "    // '{}' is not a 20-byte hex address, so there is nothing to cast.\n    \
                 address constant TARGET = address(0);",
                t.address
            ),
            false,
        )
    };
    let guard = if usable {
        r#"    function testTargetHasCode() public view {
        assertTrue(TARGET.code.length > 0, "no code at CA — wrong chain?");
    }"#
    } else {
        r#"    function testTargetHasCode() public view {
        fail("CA is not a hex address — fix it with `scout state` or re-pull before testing");
    }"#
    };
    format!(
        r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
import {{Test}} from "forge-std/Test.sol";

// Local fork only. Do not broadcast.
//   forge test --fork-url {rpc} -vv
contract HypothesisTest is Test {{
{line}

    function setUp() public {{
        // vm.createSelectFork("{rpc}");  // not needed when you pass --fork-url
    }}

{guard}

    function testHypothesis() public {{
        {PLACEHOLDER}
    }}
}}
"#
    )
}

/// True while testHypothesis is still the generated stub. Proving nothing must
/// never be allowed to flip a target to "proven".
pub fn hypothesis_is_placeholder(dir: &Path) -> bool {
    let p = dir.join("poc/test/Hypothesis.t.sol");
    let Ok(body) = std::fs::read_to_string(p) else {
        return true;
    };
    if !body.contains(PLACEHOLDER) {
        // Edited but could still be an empty body. Look for a body that is
        // only comments and whitespace.
        let start = body
            .find("function testHypothesis")
            .map(|i| i + body[i..].find('{').map(|j| j + 1).unwrap_or(0));
        if let Some(s) = start {
            let rest = &body[s..];
            let stripped: String = rest
                .lines()
                .take_while(|l| !l.trim_start().starts_with('}'))
                .map(|l| l.split("//").next().unwrap_or("").trim().to_string())
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join("");
            if !stripped.is_empty() {
                return false;
            }
        }
        return true;
    }
    true
}

pub fn print_test_cmds(dir: &Path, t: &Target) -> Result<()> {
    let poc = dir.join("poc");
    let rpc = match rpc_for(&t.chain) {
        Some(r) => r,
        None => {
            println!("no known public rpc for chain '{}'.", t.chain);
            println!("set one:  $env:SCOUT_RPC = 'https://...'");
            println!();
            println!("poc {}", poc.display());
            return Ok(());
        }
    };
    println!("poc {}", poc.display());
    println!("rpc {rpc}");
    println!();
    println!("cd {}", poc.display());
    println!("forge install foundry-rs/forge-std --no-git");
    println!("forge test --fork-url {rpc} -vv");
    println!();
    println!("no --broadcast. fork only. public rpcs rate limit; SCOUT_RPC overrides.");
    Ok(())
}

pub fn forge_bin() -> Option<String> {
    for n in ["forge", "forge.exe"] {
        if Command::new(n)
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return Some(n.to_string());
        }
    }
    // Installed but not on PATH in this shell.
    if let Some(home) = dirs::home_dir() {
        let direct = home.join(".foundry/bin/forge.exe");
        if direct.exists() {
            return Some(direct.display().to_string());
        }
    }
    None
}

pub fn prove(dir: &Path, t: &Target) -> Result<i32> {
    let poc = dir.join("poc");
    print_test_cmds(dir, t)?;
    let Some(rpc) = rpc_for(&t.chain) else {
        anyhow::bail!(
            "no rpc for chain '{}' — set SCOUT_RPC to a fork url",
            t.chain
        );
    };
    let Some(forge) = forge_bin() else {
        println!();
        println!("forge is not on PATH.");
        println!("Windows: download foundry_vX_win32_amd64.zip from");
        println!("         https://github.com/foundry-rs/foundry/releases");
        println!("         extract forge.exe, then add that folder to Path.");
        println!("         (foundryup does not run in PowerShell.)");
        return Ok(127);
    };
    if !poc.join("foundry.toml").exists() {
        anyhow::bail!("no poc/foundry.toml — run: scout open {}", t.id);
    }
    if !poc.join("lib/forge-std").exists() {
        println!("installing forge-std...");
        // --no-git: the poc folder is not a git repo, so the default submodule
        // install fails. (--no-commit was a silent no-op and did nothing.)
        let st = Command::new(&forge)
            .current_dir(&poc)
            .args(["install", "foundry-rs/forge-std", "--no-git"])
            .status()
            .context("forge install")?;
        if !st.success() {
            println!("forge install failed — run it by hand in {}", poc.display());
        }
    }
    println!("running fork test (no broadcast)...");
    let st = Command::new(&forge)
        .current_dir(&poc)
        .args(["test", "--fork-url", &rpc, "-vv"])
        .status()
        .context("forge test")?;
    Ok(st.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(chain: &str, address: &str) -> Target {
        Target {
            id: 1,
            chain: chain.into(),
            address: address.into(),
            name: "t".into(),
            deployer: String::new(),
            pair: String::new(),
            url: String::new(),
            source: String::new(),
            first_seen: String::new(),
            score: 0,
            state: "seen".into(),
            signals: String::new(),
            heat: String::new(),
            severity: String::new(),
            contract: String::new(),
        }
    }

    #[test]
    fn unknown_chain_has_no_rpc_instead_of_a_dead_localhost() {
        assert_eq!(
            rpc_for("base"),
            Some("https://mainnet.base.org".to_string())
        );
        assert_eq!(
            rpc_for("ethereum"),
            Some("https://eth.llamarpc.com".to_string())
        );
        assert_eq!(rpc_for("tezos"), None);
        assert_eq!(rpc_for(""), None);
    }

    #[test]
    fn hex_address_validation() {
        assert!(is_hex_address("0x613db9c924adb7a7a90c11b02cce20c913e3fe50"));
        assert!(is_hex_address("0x613DB9C924ADB7A7A90C11B02CCE20C913E3FE50"));
        assert!(!is_hex_address(
            "So11111111111111111111111111111111111111112"
        ));
        assert!(!is_hex_address("0x1234"));
        assert!(!is_hex_address("not-an-address"));
    }

    #[test]
    fn scaffold_stays_valid_solidity_for_a_non_hex_ca() {
        let t = target("base", "So11111111111111111111111111111111111111112");
        let s = fork_test_sol(&t);
        assert!(s.contains("fail("), "must fail loudly, not silently pass");
        assert!(!s.contains("address constant TARGET = So1"), "{s}");
    }

    #[test]
    fn placeholder_detection_on_fresh_scaffold() {
        let d = std::env::temp_dir().join("scout_poc_placeholder");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("poc/test")).unwrap();
        std::fs::write(
            d.join("poc/test/Hypothesis.t.sol"),
            fork_test_sol(&target(
                "base",
                "0x613db9c924adb7a7a90c11b02cce20c913e3fe50",
            )),
        )
        .unwrap();
        assert!(hypothesis_is_placeholder(&d));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn placeholder_detection_after_the_user_writes_a_test() {
        let d = std::env::temp_dir().join("scout_poc_filled");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("poc/test")).unwrap();
        let filled = r#"// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;
import {Test} from "forge-std/Test.sol";
contract HypothesisTest is Test {
    function testHypothesis() public {
        vm.prank(stranger);
        ITarget(TARGET).withdrawEthToUser(hacker, address(this).balance);
        assertEq(address(hacker).balance, expected);
    }
}
"#;
        std::fs::write(d.join("poc/test/Hypothesis.t.sol"), filled).unwrap();
        assert!(!hypothesis_is_placeholder(&d));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn placeholder_detection_when_the_marker_is_removed_but_body_is_empty() {
        let d = std::env::temp_dir().join("scout_poc_emptied");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("poc/test")).unwrap();
        let emptied = r#"pragma solidity ^0.8.24;
contract HypothesisTest is Test {
    function testHypothesis() public {
        // nothing yet
    }
}
"#;
        std::fs::write(d.join("poc/test/Hypothesis.t.sol"), emptied).unwrap();
        assert!(
            hypothesis_is_placeholder(&d),
            "an empty body still proves nothing"
        );
        let _ = std::fs::remove_dir_all(&d);
    }
}
