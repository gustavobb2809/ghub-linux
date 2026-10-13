//! Cliente D-Bus real (zbus bloqueante) do ghubd.
//!
//! Toda chamada de metodo tem timeout limitado (`CALL_TIMEOUT`) e, antes de
//! chamar, verificamos se o nome do daemon tem dono: sem dono devolvemos
//! `ApiError::Unavailable` na hora (e evitamos a auto-ativacao implicita do
//! barramento, que e feita explicitamente por `start_daemon`).

use super::controller::{Event, Msg};
use super::{ApiError, DaemonApi};
use ghub_ipc::{DaemonProxyBlocking, DeviceState, DeviceSummary, LedSetting, BUS_NAME, INTERFACE};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::time::Duration;
use zbus::blocking::fdo::DBusProxy;
use zbus::blocking::Connection;
use zbus::message::Type as MsgType;
use zbus::MatchRule;

/// Limite por chamada de metodo D-Bus.
pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);

/// Interpreta a saida de `systemctl --user is-enabled`.
pub fn parse_is_enabled(out: &str) -> Option<bool> {
    match out.trim_end_matches('\n') {
        "enabled" | "enabled-runtime" => Some(true),
        "disabled" => Some(false),
        _ => None,
    }
}

/// Classificacao pura de um erro de metodo D-Bus.
pub fn classify(name: &str, desc: Option<&str>) -> ApiError {
    match name {
        "org.freedesktop.DBus.Error.ServiceUnknown" | "org.freedesktop.DBus.Error.NameHasNoOwner" => {
            ApiError::Unavailable
        }
        _ => ApiError::Failed(match desc {
            Some(d) if !d.is_empty() => d.to_string(),
            _ => name.to_string(),
        }),
    }
}

pub fn map_error(e: zbus::Error) -> ApiError {
    match &e {
        zbus::Error::MethodError(name, desc, _) => classify(name.as_str(), desc.as_deref()),
        zbus::Error::InputOutput(io) if io.kind() == std::io::ErrorKind::TimedOut => {
            ApiError::Failed("O ghubd não respondeu a tempo.".to_string())
        }
        zbus::Error::InputOutput(_) => ApiError::Unavailable,
        _ => ApiError::Failed(e.to_string()),
    }
}

pub struct RealDaemon {
    conn: Connection,
}

impl RealDaemon {
    pub fn connect() -> Result<RealDaemon, zbus::Error> {
        let conn = zbus::blocking::connection::Builder::session()?
            .method_timeout(CALL_TIMEOUT)
            .build()?;
        Ok(RealDaemon { conn })
    }

    fn has_owner(&self) -> bool {
        let Ok(name) = BUS_NAME.try_into() else { return false };
        DBusProxy::new(&self.conn)
            .map_err(|e| e.to_string()).and_then(|p| p.name_has_owner(name).map_err(|e| e.to_string()))
            .unwrap_or(false)
    }

    /// Executa uma chamada no proxy; sem dono do nome => `Unavailable` imediato.
    fn call<T>(&self, f: impl FnOnce(&DaemonProxyBlocking<'_>) -> zbus::Result<T>) -> Result<T, ApiError> {
        if !self.has_owner() {
            return Err(ApiError::Unavailable);
        }
        let proxy = DaemonProxyBlocking::new(&self.conn).map_err(map_error)?;
        f(&proxy).map_err(map_error)
    }

    /// Inicia threads de fundo que traduzem sinais do daemon em `Msg::Event`.
    /// Encerram quando o receptor e descartado (sem panico) ou quando a
    /// conexao falha. Precisa de um barramento de sessao (sem teste automatico).
    pub fn spawn_signal_listener(&self, tx: Sender<Msg>) -> Result<(), zbus::Error> {
        spawn_signal_listener_on(&self.conn, tx)
    }
}

/// Cria uma conexao propria e inicia os listeners de sinais.
pub fn spawn_signal_listener(tx: Sender<Msg>) -> Result<(), zbus::Error> {
    let conn = Connection::session()?;
    spawn_signal_listener_on(&conn, tx)
}

fn spawn_err(e: std::io::Error) -> zbus::Error {
    zbus::Error::Failure(format!("não foi possível criar a thread de sinais: {e}"))
}

fn spawn_signal_listener_on(conn: &Connection, tx: Sender<Msg>) -> Result<(), zbus::Error> {
    // Regras e proxies sao criados aqui para que erros voltem ao chamador.
    let rule = MatchRule::builder()
        .msg_type(MsgType::Signal)
        .sender(BUS_NAME)?
        .interface(INTERFACE)?
        .build();
    let iter = zbus::blocking::MessageIterator::for_match_rule(rule, conn, None)?;
    let owner_changes = DBusProxy::new(conn)?.receive_name_owner_changed()?;

    let tx1 = tx.clone();
    std::thread::Builder::new()
        .name("ghub-signals".into())
        .spawn(move || {
            for msg in iter {
                let Ok(msg) = msg else { break };
                let header = msg.header();
                let Some(member) = header.member() else { continue };
                let event = match member.as_str() {
                    "DevicesChanged" => Some(Event::DevicesChanged),
                    "DeviceChanged" => msg.body().deserialize::<String>().ok().map(Event::DeviceChanged),
                    "ProfileChanged" => msg.body().deserialize::<String>().ok().map(Event::ProfileChanged),
                    _ => None,
                };
                if let Some(ev) = event {
                    if tx1.send(Msg::Event(ev)).is_err() {
                        break;
                    }
                }
            }
        })
        .map_err(spawn_err)?;

    std::thread::Builder::new()
        .name("ghub-owner".into())
        .spawn(move || {
            for sig in owner_changes {
                let Ok(args) = sig.args() else { continue };
                if args.name().as_str() != BUS_NAME {
                    continue;
                }
                let up = args.new_owner().is_some();
                if tx.send(Msg::Event(Event::OwnerChanged(up))).is_err() {
                    break;
                }
            }
        })
        .map_err(spawn_err)?;
    Ok(())
}

fn systemctl_failure(out: &std::process::Output) -> String {
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if err.is_empty() {
        format!("systemctl saiu com {}", out.status)
    } else {
        err
    }
}

impl DaemonApi for RealDaemon {
    fn daemon_running(&self) -> bool {
        self.has_owner()
    }

    fn start_daemon(&self) -> Result<(), String> {
        let fail = |e: String| {
            format!("Não foi possível iniciar o serviço: {e}. Tente: systemctl --user start ghubd")
        };
        let name = BUS_NAME.try_into().map_err(|e: zbus::names::Error| fail(e.to_string()))?;
        DBusProxy::new(&self.conn)
            .map_err(|e| e.to_string()).and_then(|p| p.start_service_by_name(name, 0).map_err(|e| e.to_string()))
            .map(|_| ())
            .map_err(fail)
    }

    fn devices(&self) -> Result<Vec<DeviceSummary>, ApiError> {
        self.call(|p| p.devices())
    }
    fn get_device_state(&self, id: &str) -> Result<DeviceState, ApiError> {
        self.call(|p| p.get_device_state(id))
    }
    fn set_dpi_levels(&self, id: &str, levels: Vec<u32>, active: u32) -> Result<(), ApiError> {
        self.call(|p| p.set_dpi_levels(id, levels, active))
    }
    fn set_active_dpi_level(&self, id: &str, active: u32) -> Result<(), ApiError> {
        self.call(|p| p.set_active_dpi_level(id, active))
    }
    fn set_polling_rate(&self, id: &str, hz: u32) -> Result<(), ApiError> {
        self.call(|p| p.set_polling_rate(id, hz))
    }
    fn led_effects(&self, id: &str) -> Result<Vec<String>, ApiError> {
        self.call(|p| p.led_effects(id))
    }
    fn get_led(&self, id: &str) -> Result<LedSetting, ApiError> {
        self.call(|p| p.get_led(id))
    }
    fn set_led(&self, id: &str, s: &LedSetting) -> Result<(), ApiError> {
        self.call(|p| p.set_led(id, &s.effect, &s.color, s.period_ms, s.intensity))
    }
    fn list_profiles(&self) -> Result<Vec<String>, ApiError> {
        self.call(|p| p.list_profiles())
    }
    fn get_active_profile(&self) -> Result<String, ApiError> {
        self.call(|p| p.get_active_profile())
    }
    fn activate_profile(&self, name: &str) -> Result<(), ApiError> {
        self.call(|p| p.activate_profile(name))
    }
    fn create_profile(&self, name: &str) -> Result<(), ApiError> {
        self.call(|p| p.create_profile(name))
    }
    fn delete_profile(&self, name: &str) -> Result<(), ApiError> {
        self.call(|p| p.delete_profile(name))
    }
    fn rescan(&self) -> Result<(), ApiError> {
        self.call(|p| p.rescan())
    }

    fn autostart(&self) -> Option<bool> {
        let out = Command::new("systemctl")
            .args(["--user", "is-enabled", "ghubd.service"])
            .output()
            .ok()?;
        parse_is_enabled(&String::from_utf8_lossy(&out.stdout))
    }

    fn set_autostart(&self, enable: bool) -> Result<(), String> {
        let verb = if enable { "enable" } else { "disable" };
        let out = Command::new("systemctl")
            .args(["--user", verb, "ghubd.service"])
            .output()
            .map_err(|e| format!("Não foi possível executar systemctl: {e}"))?;
        if out.status.success() { Ok(()) } else { Err(systemctl_failure(&out)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_enabled_parsing() {
        assert_eq!(parse_is_enabled("enabled\n"), Some(true));
        assert_eq!(parse_is_enabled("enabled-runtime\n"), Some(true));
        assert_eq!(parse_is_enabled("disabled\n"), Some(false));
        for other in ["", "static\n", "masked\n", "not-found\n", "x"] {
            assert_eq!(parse_is_enabled(other), None);
        }
    }

    #[test]
    fn service_unknown_maps_to_unavailable() {
        assert_eq!(classify("org.freedesktop.DBus.Error.ServiceUnknown", None), ApiError::Unavailable);
        assert_eq!(classify("org.freedesktop.DBus.Error.NameHasNoOwner", Some("x")), ApiError::Unavailable);
        assert_eq!(
            classify("org.freedesktop.DBus.Error.InvalidArgs", Some("DPI inválido")),
            ApiError::Failed("DPI inválido".into())
        );
        assert_eq!(
            classify("org.freedesktop.DBus.Error.Failed", None),
            ApiError::Failed("org.freedesktop.DBus.Error.Failed".into())
        );
    }

    #[test]
    fn io_errors_map() {
        use std::io::{Error, ErrorKind};
        let t = map_error(zbus::Error::InputOutput(Error::from(ErrorKind::TimedOut).into()));
        assert!(matches!(t, ApiError::Failed(_)));
        let b = map_error(zbus::Error::InputOutput(Error::from(ErrorKind::BrokenPipe).into()));
        assert_eq!(b, ApiError::Unavailable);
        assert!(matches!(map_error(zbus::Error::NameTaken), ApiError::Failed(_)));
    }

    #[test]
    #[ignore = "needs a session bus"]
    fn connect_and_probe_without_panicking() {
        let d = RealDaemon::connect().expect("session bus");
        let _ = d.daemon_running();
        let _ = d.devices(); // Ok ou Unavailable, nunca bloqueia alem do timeout
        let (tx, _rx) = std::sync::mpsc::channel();
        d.spawn_signal_listener(tx).expect("listener");
    }
}
