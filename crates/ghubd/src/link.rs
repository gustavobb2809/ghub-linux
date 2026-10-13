use std::process::Command as Process;
use std::sync::mpsc::{channel, Sender};
use std::time::Duration;

use ghub_ipc::{DeviceState, DeviceSummary};

use crate::manager::{Command, ManagerError};
use crate::tray_model::Autostart;

/// Unidade do systemd de usuário que roda o daemon.
pub const UNIT: &str = "ghubd.service";
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// O que a bandeja precisa do gerente de dispositivos.
pub trait ManagerLink: Send + Sync {
    fn devices(&self) -> Vec<DeviceSummary>;
    fn state(&self, id: &str) -> Option<DeviceState>;
    fn profiles(&self) -> Vec<String>;
    fn active_profile(&self) -> String;
    fn set_active_level(&self, id: &str, level: usize) -> Result<(), ManagerError>;
    fn activate_profile(&self, name: &str) -> Result<(), ManagerError>;
    /// Pede ao gerente que pare (fim do laço principal).
    fn shutdown(&self);
}

/// Fala com o gerente pelos mesmos canais que o serviço D-Bus usa.
pub struct ChannelLink {
    tx: Sender<Command>,
    timeout: Duration,
}

impl ChannelLink {
    pub fn new(tx: Sender<Command>) -> Self {
        Self::with_timeout(tx, REPLY_TIMEOUT)
    }

    pub fn with_timeout(tx: Sender<Command>, timeout: Duration) -> Self {
        ChannelLink { tx, timeout }
    }

    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> Command) -> Option<T> {
        let (reply_tx, reply_rx) = channel();
        self.tx.send(make(reply_tx)).ok()?;
        reply_rx.recv_timeout(self.timeout).ok()
    }

    fn unreachable() -> ManagerError {
        ManagerError::Device("o gerente de dispositivos não respondeu".into())
    }
}

impl ManagerLink for ChannelLink {
    fn devices(&self) -> Vec<DeviceSummary> {
        self.ask(Command::Devices).unwrap_or_default()
    }

    fn state(&self, id: &str) -> Option<DeviceState> {
        let id = id.to_string();
        self.ask(|reply| Command::State { id, reply })?.ok()
    }

    fn profiles(&self) -> Vec<String> {
        self.ask(Command::ListProfiles).unwrap_or_default()
    }

    fn active_profile(&self) -> String {
        self.ask(Command::ActiveProfile).unwrap_or_default()
    }

    fn set_active_level(&self, id: &str, level: usize) -> Result<(), ManagerError> {
        let id = id.to_string();
        self.ask(|reply| Command::SetActiveLevel { id, active: level, reply })
            .unwrap_or_else(|| Err(Self::unreachable()))
    }

    fn activate_profile(&self, name: &str) -> Result<(), ManagerError> {
        let name = name.to_string();
        self.ask(|reply| Command::ActivateProfile { name, reply })
            .unwrap_or_else(|| Err(Self::unreachable()))
    }

    fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown);
    }
}

/// Interpreta a saída de `systemctl --user is-enabled`. Qualquer coisa que não seja
/// "enabled"/"disabled" (serviço ausente, estático, mascarado…) conta como indisponível.
pub fn parse_is_enabled(stdout: &str) -> Autostart {
    match stdout.trim() {
        "enabled" | "enabled-runtime" => Autostart::Enabled,
        "disabled" => Autostart::Disabled,
        _ => Autostart::Unavailable,
    }
}

/// Diz se este processo é o processo principal da unidade do systemd. O systemd define
/// `INVOCATION_ID` e `SYSTEMD_EXEC_PID` para o processo que ele inicia, mas `INVOCATION_ID`
/// é herdado por tudo o que for lançado de dentro de uma unidade (terminal, compositor…);
/// por isso exige também que `SYSTEMD_EXEC_PID` seja o PID deste processo.
pub fn runs_under_systemd(invocation_id: Option<&str>, exec_pid: Option<&str>, own_pid: u32) -> bool {
    invocation_id.is_some_and(|v| !v.is_empty())
        && exec_pid.and_then(|p| p.trim().parse::<u32>().ok()) == Some(own_pid)
}

pub trait ServiceControl: Send + Sync {
    fn autostart(&self) -> Autostart;
    fn set_autostart(&self, enable: bool) -> Result<(), String>;
    /// Para o serviço. `Ok(true)`: pediu ao systemd (o systemd encerra o processo);
    /// `Ok(false)`: este processo não é o processo principal do serviço, quem chama deve
    /// encerrar por conta própria.
    fn stop(&self) -> Result<bool, String>;
}

/// Texto de erro de uma falha do `systemctl`: o stderr aparado ou, se vazio, o status de saída.
fn failure_text(stderr: &str, status: &str) -> String {
    match stderr.trim() {
        "" => format!("systemctl saiu com {status}"),
        text => text.to_string(),
    }
}

pub struct SystemctlControl;

fn systemctl(args: &[&str]) -> std::io::Result<std::process::Output> {
    Process::new("systemctl").arg("--user").args(args).output()
}

impl ServiceControl for SystemctlControl {
    fn autostart(&self) -> Autostart {
        match systemctl(&["is-enabled", UNIT]) {
            Ok(out) => parse_is_enabled(&String::from_utf8_lossy(&out.stdout)),
            Err(_) => Autostart::Unavailable,
        }
    }

    fn set_autostart(&self, enable: bool) -> Result<(), String> {
        let verb = if enable { "enable" } else { "disable" };
        let out = systemctl(&[verb, UNIT]).map_err(|e| format!("não foi possível executar o systemctl: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(failure_text(&String::from_utf8_lossy(&out.stderr), &out.status.to_string()))
        }
    }

    fn stop(&self) -> Result<bool, String> {
        let id = std::env::var("INVOCATION_ID").ok();
        let exec_pid = std::env::var("SYSTEMD_EXEC_PID").ok();
        if !runs_under_systemd(id.as_deref(), exec_pid.as_deref(), std::process::id()) {
            return Ok(false);
        }
        let out = systemctl(&["stop", "--no-block", UNIT])
            .map_err(|e| format!("não foi possível executar o systemctl: {e}"))?;
        if out.status.success() {
            Ok(true)
        } else {
            Err(failure_text(&String::from_utf8_lossy(&out.stderr), &out.status.to_string()))
        }
    }
}

pub trait Launcher: Send + Sync {
    fn open_gui(&self) -> Result<(), String>;
}

/// Texto máximo de stderr guardado do filho.
const STDERR_LIMIT: usize = 2048;
/// Quanto tempo o filho tem para falhar antes de o lançamento ser dado como bem-sucedido.
const LAUNCH_WINDOW: Duration = Duration::from_millis(1500);
const POLL_STEP: Duration = Duration::from_millis(50);
const NOT_INSTALLED: &str = "a interface gráfica (ghub-gui) ainda não está instalada";

/// Argumentos do `systemd-run`: a GUI roda num escopo transitório próprio, fora do cgroup do
/// daemon (herda o ambiente gráfico da sessão e sobrevive a um restart do `ghubd`).
pub(crate) fn scope_args() -> [&'static str; 5] {
    ["--user", "--scope", "--collect", "--quiet", "ghub-gui"]
}

/// Texto legível para uma falha de lançamento, nunca vazio.
pub(crate) fn launch_failure_text(stderr: &str, status: &str) -> String {
    if stderr.contains("No such file") {
        return NOT_INSTALLED.into();
    }
    match stderr.lines().map(str::trim).find(|l| !l.is_empty()) {
        Some(line) => line.into(),
        None => format!("a interface saiu com {status}"),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Watch {
    /// O programa não existe.
    NotFound,
    /// Falhou ao iniciar ou saiu com erro logo no começo (texto já legível).
    Failed(String),
}

/// Inicia `program`, acompanha-o por `window` e diz se ele falhou cedo. Saída com sucesso ou
/// ainda rodando ao fim da janela é `Ok`; nesse caso uma thread recolhe o filho depois.
fn spawn_and_watch(program: &str, args: &[&str], window: Duration) -> Result<(), Watch> {
    use std::io::Read;
    use std::process::Stdio;
    use std::sync::{Arc, Mutex};

    let mut child = Process::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => Watch::NotFound,
            _ => Watch::Failed(format!("não foi possível abrir a interface: {e}")),
        })?;

    // Drena o stderr até o EOF (para o filho nunca travar na escrita), guardando só o começo.
    let captured = Arc::new(Mutex::new(Vec::<u8>::new()));
    let (eof_tx, eof_rx) = channel::<()>();
    if let Some(mut stderr) = child.stderr.take() {
        let captured = Arc::clone(&captured);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 512];
            while let Ok(n) = stderr.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                if let Ok(mut buf) = captured.lock() {
                    let room = STDERR_LIMIT.saturating_sub(buf.len());
                    buf.extend_from_slice(&chunk[..n.min(room)]);
                }
            }
            let _ = eof_tx.send(());
        });
    }

    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                // Dá um instante à thread de drenagem para chegar ao EOF.
                let _ = eof_rx.recv_timeout(Duration::from_millis(300));
                let text = captured.lock().map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                return Err(Watch::Failed(launch_failure_text(&text, &status.to_string())));
            }
            Ok(None) if started.elapsed() >= window => break,
            Ok(None) => std::thread::sleep(POLL_STEP),
            Err(e) => return Err(Watch::Failed(format!("não foi possível acompanhar a interface: {e}"))),
        }
    }
    // Ainda rodando: recolhe o processo depois, para não deixar zumbi.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Lança `program` direto, com texto legível para o `NotFound`.
fn launch_direct(program: &str, args: &[&str], window: Duration) -> Result<(), String> {
    spawn_and_watch(program, args, window).map_err(|w| match w {
        Watch::NotFound => NOT_INSTALLED.to_string(),
        Watch::Failed(t) => t,
    })
}

/// Abre a interface gráfica (`ghub-gui`) num escopo transitório do systemd de usuário; sem
/// `systemd-run`, cai para o lançamento direto.
pub struct GuiLauncher;

impl Launcher for GuiLauncher {
    fn open_gui(&self) -> Result<(), String> {
        match spawn_and_watch("systemd-run", &scope_args(), LAUNCH_WINDOW) {
            Err(Watch::NotFound) => launch_direct("ghub-gui", &[], LAUNCH_WINDOW),
            Err(Watch::Failed(t)) => Err(t),
            Ok(()) => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::Manager;
    use crate::profile::ProfileStore;
    use crate::testing::{FakeBackend, FakeMouse};
    use std::thread;

    #[test]
    fn scope_command_runs_the_gui_in_its_own_scope() {
        assert_eq!(scope_args(), ["--user", "--scope", "--collect", "--quiet", "ghub-gui"]);
    }

    #[test]
    fn launch_failure_text_is_readable_and_never_empty() {
        assert!(launch_failure_text("Failed to execute ghub-gui: No such file or directory", "exit status: 1")
            .contains("ainda não está instalada"));
        assert_eq!(
            launch_failure_text("qt.qpa.xcb: could not connect to display\nmais\n", "exit status: 1"),
            "qt.qpa.xcb: could not connect to display"
        );
        assert_eq!(launch_failure_text("  \n", "exit status: 3"), "a interface saiu com exit status: 3");
    }

    // Os testes abaixo usam comandos inofensivos (`sh`, `true`, `sleep`), nunca o systemd nem a GUI.
    const WINDOW: Duration = Duration::from_millis(1500);

    #[test]
    fn early_failure_reports_stderr_text() {
        let r = spawn_and_watch("sh", &["-c", "echo erro-de-teste >&2; exit 3"], WINDOW);
        assert_eq!(r, Err(Watch::Failed("erro-de-teste".into())));
    }

    #[test]
    fn early_failure_without_stderr_reports_the_status() {
        match spawn_and_watch("sh", &["-c", "exit 3"], WINDOW) {
            Err(Watch::Failed(t)) => assert!(t.starts_with("a interface saiu com"), "{t}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn early_success_is_ok() {
        assert_eq!(spawn_and_watch("true", &[], WINDOW), Ok(()));
    }

    #[test]
    fn still_running_after_the_window_is_ok() {
        let start = std::time::Instant::now();
        let r = spawn_and_watch("sleep", &["3"], Duration::from_millis(200));
        assert_eq!(r, Ok(()));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn noisy_stderr_is_bounded_and_does_not_hang_the_child() {
        // ~200 KiB em stderr: o filho não pode travar na escrita e o texto fica limitado.
        let r = spawn_and_watch("sh", &["-c", "head -c 200000 /dev/zero | tr '\\0' x >&2; exit 1"], WINDOW);
        match r {
            Err(Watch::Failed(t)) => assert!(t.len() <= STDERR_LIMIT, "{}", t.len()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn missing_program_is_not_found_and_readable() {
        assert_eq!(spawn_and_watch("ghub-programa-que-nao-existe", &[], WINDOW), Err(Watch::NotFound));
        assert_eq!(
            launch_direct("ghub-programa-que-nao-existe", &[], WINDOW),
            Err("a interface gráfica (ghub-gui) ainda não está instalada".to_string())
        );
    }

    #[test]
    fn parses_systemctl_is_enabled_output() {
        assert_eq!(parse_is_enabled("enabled\n"), Autostart::Enabled);
        assert_eq!(parse_is_enabled("enabled-runtime\n"), Autostart::Enabled);
        assert_eq!(parse_is_enabled("disabled\n"), Autostart::Disabled);
        for other in ["", "static\n", "masked\n", "linked\n", "not-found\n", "garbage"] {
            assert_eq!(parse_is_enabled(other), Autostart::Unavailable, "{other:?}");
        }
    }

    #[test]
    fn detects_when_running_under_systemd() {
        assert!(runs_under_systemd(Some("abc123"), Some("4242"), 4242));
        assert!(!runs_under_systemd(Some(""), Some("4242"), 4242));
        assert!(!runs_under_systemd(None, Some("4242"), 4242));
    }

    #[test]
    fn systemctl_failure_text_is_never_empty() {
        assert_eq!(failure_text("  Unit ghubd.service not found.\n", "exit status: 1"), "Unit ghubd.service not found.");
        assert_eq!(failure_text("", "exit status: 1"), "systemctl saiu com exit status: 1");
        assert_eq!(failure_text(" \n\t", "exit status: 5"), "systemctl saiu com exit status: 5");
    }

    #[test]
    fn an_inherited_invocation_id_is_not_enough() {
        // Iniciado à mão a partir de um terminal/compositor que roda numa unidade do systemd.
        assert!(!runs_under_systemd(Some("abc123"), None, 4242));
        assert!(!runs_under_systemd(Some("abc123"), Some("1000"), 4242));
        assert!(!runs_under_systemd(Some("abc123"), Some("lixo"), 4242));
        assert!(!runs_under_systemd(Some("abc123"), Some(""), 4242));
        assert!(!runs_under_systemd(None, None, 4242));
    }

    fn running_link() -> (ChannelLink, Sender<Command>, thread::JoinHandle<()>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::from_toml(
            "active = \"Padrão\"\n[[profile]]\nname = \"Padrão\"\ndpi_levels = [800, 1600]\nactive_level = 1\npolling_hz = 1000\n",
        )
        .unwrap();
        let (backend, _bat) = FakeBackend::with_mouse(FakeMouse::new(1600, 1000));
        let (ev_tx, _ev_rx) = channel();
        let manager = Manager::new(backend, store, dir.path().join("profiles.toml"), ev_tx);
        let (tx, rx) = channel();
        let handle = thread::spawn(move || manager.run(rx, Duration::from_secs(60)));
        (ChannelLink::new(tx.clone()), tx, handle, dir)
    }

    #[test]
    fn channel_link_reads_and_writes_through_the_manager() {
        let (link, tx, handle, _d) = running_link();
        let devices = link.devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "dev-1");
        assert_eq!(link.state("dev-1").unwrap().dpi_current, 1600);
        assert!(link.state("nope").is_none());
        assert_eq!(link.profiles(), vec!["Padrão".to_string()]);
        assert_eq!(link.active_profile(), "Padrão");
        link.set_active_level("dev-1", 0).unwrap();
        assert_eq!(link.state("dev-1").unwrap().dpi_current, 800);
        assert!(matches!(
            link.set_active_level("dev-1", 9),
            Err(ManagerError::Invalid(_))
        ));
        assert!(matches!(
            link.activate_profile("zzz"),
            Err(ManagerError::NotFound(_))
        ));
        link.activate_profile("Padrão").unwrap();
        link.shutdown();
        handle.join().unwrap();
        drop(tx);
    }

    #[test]
    fn channel_link_degrades_gracefully_when_the_manager_is_gone() {
        let (link, tx, handle, _d) = running_link();
        link.shutdown();
        handle.join().unwrap();
        drop(tx);
        assert!(link.devices().is_empty());
        assert!(link.state("dev-1").is_none());
        assert!(link.profiles().is_empty());
        assert_eq!(link.active_profile(), "");
        assert!(matches!(
            link.set_active_level("dev-1", 0),
            Err(ManagerError::Device(_))
        ));
    }

    #[test]
    fn channel_link_times_out_instead_of_hanging() {
        let (tx, _rx_kept_alive) = channel::<Command>();
        let link = ChannelLink::with_timeout(tx, Duration::from_millis(50));
        assert!(link.devices().is_empty());
        assert!(matches!(
            link.activate_profile("x"),
            Err(ManagerError::Device(_))
        ));
    }
}
