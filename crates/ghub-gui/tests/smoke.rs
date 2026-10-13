#![cfg(all(feature = "qt", feature = "fake-daemon"))]
use std::process::Command;

fn run(smoke: &str, fake: Option<&str>) -> i32 {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ghub-gui"));
    c.args(["--smoke", smoke]).env("QT_QPA_PLATFORM", "offscreen");
    if let Some(f) = fake {
        c.args(["--fake", f]);
    }
    c.status().expect("ghub-gui deve iniciar").code().unwrap_or(-1)
}

#[test]
fn theme_singleton_resolves() {
    assert_eq!(run("theme", Some("running")), 0);
}

#[test]
fn counter_proof_wrong_scenario_fails() {
    assert_eq!(run("fail", Some("running")), 7);
}

#[test]
fn running_scenario_reaches_the_property_from_another_thread() {
    assert_eq!(run("running", Some("running")), 0);
}

#[test]
fn stopped_scenario() {
    assert_eq!(run("stopped", Some("stopped")), 0);
}

#[test]
fn sleeping_scenario() {
    assert_eq!(run("sleeping", Some("sleeping")), 0);
}

#[test]
fn counter_proof_running_smoke_fails_when_daemon_is_stopped() {
    assert_eq!(run("running", Some("stopped")), 7);
}

#[test]
fn edit_scenario_updates_optimistically_and_validates() {
    assert_eq!(run("edit", Some("running")), 0);
}

#[test]
fn counter_proof_edit_fails_without_a_device() {
    assert_eq!(run("edit", Some("stopped")), 7);
}

#[test]
fn counter_proof_sleeping_smoke_fails_when_mouse_is_awake() {
    assert_eq!(run("sleeping", Some("running")), 7);
}

#[test]
fn counter_proof_stopped_smoke_fails_when_daemon_runs() {
    assert_eq!(run("stopped", Some("running")), 7);
}
