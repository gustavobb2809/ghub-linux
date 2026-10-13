#![cfg(feature = "qt")]
use std::process::Command;

fn run(scenario: &str) -> i32 {
    Command::new(env!("CARGO_BIN_EXE_ghub-gui"))
        .args(["--smoke", scenario])
        .env("QT_QPA_PLATFORM", "offscreen")
        .status()
        .expect("ghub-gui deve iniciar")
        .code()
        .unwrap_or(-1)
}

#[test]
fn theme_singleton_resolves() {
    assert_eq!(run("theme"), 0);
}

#[test]
fn counter_proof_wrong_scenario_fails() {
    assert_eq!(run("fail"), 7);
}
