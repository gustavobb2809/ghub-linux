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

#[test]
fn pages_scenario_navigates_and_reaches_the_ui() {
    assert_eq!(run("pages", Some("running")), 0);
}

#[test]
fn counter_proof_pages_fails_when_daemon_is_stopped() {
    assert_eq!(run("pages", Some("stopped")), 7);
}

#[test]
fn disconnected_page_shows_when_daemon_is_stopped() {
    assert_eq!(run("disconnected", Some("stopped")), 0);
}

#[test]
fn counter_proof_disconnected_fails_when_daemon_runs() {
    assert_eq!(run("disconnected", Some("running")), 7);
}

#[test]
fn sleeping_mouse_disables_controls_and_never_shows_zero_dpi() {
    assert_eq!(run("dimmed", Some("sleeping")), 0);
}

#[test]
fn counter_proof_dimmed_fails_when_mouse_is_awake() {
    assert_eq!(run("dimmed", Some("running")), 7);
}

#[test]
fn slider_release_and_field_edit_commit_the_dpi() {
    assert_eq!(run("release", Some("running")), 0);
}

#[test]
fn counter_proof_release_fails_when_mouse_is_asleep() {
    assert_eq!(run("release", Some("sleeping")), 7);
}

#[test]
fn clicking_the_active_tab_keeps_it_checked() {
    assert_eq!(run("nav", Some("running")), 0);
}

#[test]
fn counter_proof_nav_fails_when_tabs_are_disabled() {
    assert_eq!(run("nav", Some("stopped")), 7);
}

#[test]
fn rejected_edit_keeps_the_field_following_the_model() {
    assert_eq!(run("rebind", Some("running")), 0);
}

#[test]
fn counter_proof_rebind_fails_when_mouse_is_asleep() {
    assert_eq!(run("rebind", Some("sleeping")), 7);
}

#[test]
fn qml_load_failure_exits_with_code_1_instead_of_hanging() {
    assert_eq!(run("load-failure", Some("running")), 1);
}

#[test]
fn window_is_visible_by_default() {
    assert_eq!(run("visible", Some("running")), 0);
}

#[test]
fn counter_proof_hidden_expectation_fails() {
    assert_eq!(run("hidden", Some("running")), 7);
}

#[test]
fn non_numeric_or_overflowing_dpi_text_is_rejected() {
    assert_eq!(run("badtext", Some("running")), 0);
}

#[test]
fn counter_proof_badtext_fails_when_mouse_is_asleep() {
    assert_eq!(run("badtext", Some("sleeping")), 7);
}

#[test]
fn smoke_without_fake_exits_2_for_backend_scenarios() {
    assert_eq!(run("running", None), 2);
}

#[test]
fn bare_smoke_flag_exits_2() {
    let code = Command::new(env!("CARGO_BIN_EXE_ghub-gui"))
        .arg("--smoke")
        .env("QT_QPA_PLATFORM", "offscreen")
        .status()
        .unwrap()
        .code();
    assert_eq!(code, Some(2));
}

#[test]
fn backendless_smoke_runs_without_fake() {
    assert_eq!(run("theme", None), 0);
}

#[test]
fn layout_holds_at_minimum_window_size() {
    assert_eq!(run("layout", Some("running")), 0);
}

#[test]
fn counter_proof_layout_fails_when_daemon_is_stopped() {
    assert_eq!(run("layout", Some("stopped")), 7);
}

fn shots(scenario: &str, dir: Option<&std::path::Path>) -> i32 {
    let mut c = Command::new(env!("CARGO_BIN_EXE_ghub-gui"));
    c.args(["--smoke", "shots", "--fake", scenario]).env("QT_QPA_PLATFORM", "offscreen");
    if let Some(d) = dir {
        c.arg("--shots").arg(d);
    }
    c.status().expect("ghub-gui deve iniciar").code().unwrap_or(-1)
}

#[test]
fn every_page_renders_to_a_png_at_both_sizes() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("shots-running");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(shots("running", Some(&dir)), 0);
    for size in ["1280x800", "900x600"] {
        for page in ["visao-geral", "sensibilidade", "configuracoes", "menu-perfil"] {
            let f = dir.join(format!("running-{page}-{size}.png"));
            assert!(std::fs::metadata(&f).map(|m| m.len() > 1000).unwrap_or(false), "faltou {f:?}");
        }
    }
}

#[test]
fn disconnected_shell_renders_when_daemon_is_stopped() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("shots-stopped");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(shots("stopped", Some(&dir)), 0);
    assert!(dir.join("stopped-desconectado-900x600.png").exists());
}

#[test]
fn counter_proof_shots_fail_without_a_target_directory() {
    assert_eq!(shots("running", None), 7);
}

#[test]
fn sidebar_logo_has_the_expected_size() {
    assert_eq!(run("logo", Some("running")), 0);
}

#[test]
fn counter_proof_logo_wrong_size_fails() {
    assert_eq!(run("logo-wrong-size", Some("running")), 7);
}
