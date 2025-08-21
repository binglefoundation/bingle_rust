use rust_comms::algo_ops::{AlgoOps, AlgoProviderConfig};
use std::process::Command;
use std::time::{Duration, Instant};

fn run_cmd(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program).args(args).output().map_err(|e| format!("failed to run {program}: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "command `{}` failed (code {:?}): {}",
            std::iter::once(program).chain(args.iter().cloned()).collect::<Vec<_>>().join(" "),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn is_addr_token(tok: &str) -> bool {
    if tok.len() != 58 { return false; }
    tok.chars().all(|c| matches!(c, 'A'..='Z' | '2'..='7'))
}

fn parse_funded_account(list_output: &str) -> Option<String> {
    // Heuristic: per line, pick a token that looks like an Algorand address and the largest numeric value as balance; select line with max balance
    let mut best_addr: Option<String> = None;
    let mut best_score: u128 = 0;
    for line in list_output.lines() {
        let toks: Vec<&str> = line.split_whitespace().collect();
        if toks.is_empty() { continue; }
        let mut addr: Option<&str> = None;
        let mut score: u128 = 0;
        for &t in &toks {
            if addr.is_none() && is_addr_token(t) {
                addr = Some(t);
            }
            if let Ok(v) = t.parse::<u128>() { if v > score { score = v; } }
        }
        if let Some(a) = addr { if score >= best_score { best_score = score; best_addr = Some(a.to_string()); } }
    }
    best_addr
}

pub fn ensure_localnet_accounts_funded(cfg: &AlgoProviderConfig, target_addrs: &[&str]) -> Result<(), String> {
    // Ensure algokit CLI is available and get a funding source account
    let list = run_cmd("algokit", &["goal", "account", "list"]) ?;
    let funded = parse_funded_account(&list).ok_or_else(|| "Could not determine a funded localnet account from `algokit goal account list` output".to_string())?;

    // For each target, ensure it's funded with at least some ALGOs
    for &addr in target_addrs {
        let ops = AlgoOps::new(None, Some(addr.to_string()), Some(cfg.clone()));
        let bal_opt = ops.account_balance().map_err(|e| format!("balance check failed: {e}"))?;
        let mut needs_fund = match bal_opt { Some(b) => b < 1.0, None => true };
        if needs_fund {
            // Send 900_000_000 microalgos (~900 ALGO) as per Kotlin SetupLocalnet
            let amount = "900000000";
            let _ = run_cmd("algokit", &["goal", "clerk", "send", "-a", amount, "-t", addr, "-f", &funded])?;
            // Poll until balance appears
            let deadline = Instant::now() + Duration::from_secs(20);
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(500));
                if let Ok(b) = AlgoOps::new(None, Some(addr.to_string()), Some(cfg.clone())).account_balance() {
                    if let Some(v) = b { if v > 0.0 { needs_fund = false; break; } }
                }
            }
        }
        if needs_fund {
            return Err(format!("Failed to fund address {} via algokit; ensure localnet is running and algokit has access to genesis accounts", addr));
        }
    }
    Ok(())
}
