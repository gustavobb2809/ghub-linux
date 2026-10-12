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

/// O systemd define `INVOCATION_ID` para os processos que ele inicia.
pub fn runs_under_systemd(invocation_id: Option<&str>) -> bool {
    invocation_id.is_some_and(|v| !v.is_empty())
}

pub trait ServiceControl: Send + Sync {
    fn autostart(&self) -> Autostart;
    fn set_autostart(&self, enable: bool) -> Result<(), String>;
    /// Para o serviço. `Ok(true)`: pediu ao systemd (o processo receberá SIGTERM);
    /// `Ok(false)`: o daemon não roda como serviço, quem chama deve encerrar por conta própria.
    fn stop(&self) -> Result<bool, String>;
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
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    fn stop(&self) -> Result<bool, String> {
        let id = std::env::var("INVOCATION_ID").ok();
        if !runs_under_systemd(id.as_deref()) {
            return Ok(false);
        }
        let out = systemctl(&["stop", "--no-block", UNIT])
            .map_err(|e| format!("não foi possível executar o systemctl: {e}"))?;
        if out.status.success() {
            Ok(true)
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
}

pub trait Launcher: Send + Sync {
    fn open_gui(&self) -> Result<(), String>;
}

/// Abre a interface gráfica (`ghub-gui`, do Plano 4) se ela estiver instalada.
pub struct GuiLauncher;

impl Launcher for GuiLauncher {
    fn open_gui(&self) -> Result<(), String> {
        match Process::new("ghub-gui").spawn() {
            Ok(mut child) => {
                // Recolhe o processo filho para não deixar zumbi.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err("a interface gráfica (ghub-gui) ainda não está instalada".into())
            }
            Err(e) => Err(format!("não foi possível abrir a interface: {e}")),
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
        assert!(runs_under_systemd(Some("abc123")));
        assert!(!runs_under_systemd(Some("")));
        assert!(!runs_under_systemd(None));
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
