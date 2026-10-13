use std::sync::mpsc::{channel, Sender};
use std::time::Duration;

use ghub_ipc::{DeviceState, DeviceSummary, LedSetting};
use hidpp::features::onboard::Led;
use zbus::{fdo, interface};

use crate::manager::{Command, ManagerError};

pub const REPLY_TIMEOUT: Duration = Duration::from_secs(5);
/// Gravar o LED escreve na flash do mouse e demora mais que as outras chamadas.
pub const LED_REPLY_TIMEOUT: Duration = Duration::from_secs(15);

fn to_fdo(e: ManagerError) -> fdo::Error {
    match e {
        ManagerError::Invalid(m) => fdo::Error::InvalidArgs(m),
        ManagerError::NotFound(m) => fdo::Error::UnknownObject(m),
        ManagerError::Offline(_) | ManagerError::Device(_) => fdo::Error::Failed(e.to_string()),
    }
}

fn to_u16(value: u32) -> fdo::Result<u16> {
    u16::try_from(value)
        .map_err(|_| fdo::Error::InvalidArgs(format!("valor fora do intervalo: {value}")))
}

/// Ponte entre o D-Bus e o gerente: cada chamada vira um `Command` e espera a resposta.
pub struct DaemonService {
    tx: Sender<Command>,
    timeout: Duration,
}

impl DaemonService {
    pub fn new(tx: Sender<Command>) -> Self {
        Self::with_timeout(tx, REPLY_TIMEOUT)
    }

    pub fn with_timeout(tx: Sender<Command>, timeout: Duration) -> Self {
        DaemonService { tx, timeout }
    }

    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> Command) -> fdo::Result<T> {
        let (reply_tx, reply_rx) = channel();
        self.tx
            .send(make(reply_tx))
            .map_err(|_| fdo::Error::Failed("o gerente de dispositivos parou".into()))?;
        reply_rx
            .recv_timeout(self.timeout)
            .map_err(|_| fdo::Error::Failed("o gerente de dispositivos não respondeu a tempo".into()))
    }

    fn ask_result<T>(
        &self,
        make: impl FnOnce(Sender<Result<T, ManagerError>>) -> Command,
    ) -> fdo::Result<T> {
        self.ask(make)?.map_err(to_fdo)
    }
}

#[interface(name = "org.ghublinux.Daemon1")]
impl DaemonService {
    fn devices(&self) -> fdo::Result<Vec<DeviceSummary>> {
        self.ask(Command::Devices)
    }

    fn get_device_state(&self, id: &str) -> fdo::Result<DeviceState> {
        let id = id.to_string();
        self.ask_result(|reply| Command::State { id, reply })
    }

    fn set_dpi_levels(&self, id: &str, levels: Vec<u32>, active: u32) -> fdo::Result<()> {
        let levels = levels.into_iter().map(to_u16).collect::<fdo::Result<Vec<u16>>>()?;
        let id = id.to_string();
        self.ask_result(|reply| Command::SetDpiLevels { id, levels, active: active as usize, reply })
    }

    fn set_active_dpi_level(&self, id: &str, active: u32) -> fdo::Result<()> {
        let id = id.to_string();
        self.ask_result(|reply| Command::SetActiveLevel { id, active: active as usize, reply })
    }

    fn set_polling_rate(&self, id: &str, hz: u32) -> fdo::Result<()> {
        let hz = to_u16(hz)?;
        let id = id.to_string();
        self.ask_result(|reply| Command::SetPolling { id, hz, reply })
    }

    fn led_effects(&self, id: &str) -> fdo::Result<Vec<String>> {
        let id = id.to_string();
        self.ask_result(|reply| Command::LedEffects { id, reply })
    }

    fn get_led(&self, id: &str) -> fdo::Result<LedSetting> {
        let id = id.to_string();
        let led = self.ask_result(|reply| Command::GetLed { id, reply })?;
        let (effect, color, period_ms, intensity) = led.parts();
        Ok(LedSetting {
            effect: effect.to_string(),
            color,
            period_ms: u32::from(period_ms),
            intensity: u32::from(intensity),
        })
    }

    fn set_led(&self, id: &str, effect: &str, color: &str, period_ms: u32, intensity: u32) -> fdo::Result<()> {
        let led = Led::from_parts(effect, color, period_ms, intensity).map_err(fdo::Error::InvalidArgs)?;
        let id = id.to_string();
        let (reply_tx, reply_rx) = channel();
        self.tx
            .send(Command::SetLed { id, led, reply: reply_tx })
            .map_err(|_| fdo::Error::Failed("o gerente de dispositivos parou".into()))?;
        reply_rx
            .recv_timeout(self.timeout.max(LED_REPLY_TIMEOUT))
            .map_err(|_| fdo::Error::Failed("o gerente de dispositivos não respondeu a tempo".into()))?
            .map_err(to_fdo)
    }

    fn list_profiles(&self) -> fdo::Result<Vec<String>> {
        self.ask(Command::ListProfiles)
    }

    fn get_active_profile(&self) -> fdo::Result<String> {
        self.ask(Command::ActiveProfile)
    }

    fn activate_profile(&self, name: &str) -> fdo::Result<()> {
        let name = name.to_string();
        self.ask_result(|reply| Command::ActivateProfile { name, reply })
    }

    fn create_profile(&self, name: &str) -> fdo::Result<()> {
        let name = name.to_string();
        self.ask_result(|reply| Command::CreateProfile { name, reply })
    }

    fn delete_profile(&self, name: &str) -> fdo::Result<()> {
        let name = name.to_string();
        self.ask_result(|reply| Command::DeleteProfile { name, reply })
    }

    fn rescan(&self) -> fdo::Result<()> {
        self.ask(|reply| Command::Rescan(Some(reply)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::Manager;
    use crate::profile::ProfileStore;
    use crate::testing::{FakeBackend, FakeMouse};
    use std::thread;

    fn running_service() -> (DaemonService, Sender<Command>, thread::JoinHandle<()>, tempfile::TempDir) {
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
        (DaemonService::new(tx.clone()), tx, handle, dir)
    }

    fn stop(tx: Sender<Command>, handle: thread::JoinHandle<()>) {
        tx.send(Command::Shutdown).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn lists_devices_and_state() {
        let (svc, tx, handle, _d) = running_service();
        let devices = svc.devices().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "dev-1");
        let state = svc.get_device_state("dev-1").unwrap();
        assert_eq!(state.dpi_current, 1600);
        assert_eq!(svc.get_active_profile().unwrap(), "Padrão");
        assert_eq!(svc.list_profiles().unwrap(), vec!["Padrão".to_string()]);
        stop(tx, handle);
    }

    #[test]
    fn good_writes_succeed_and_bad_input_is_invalid_args() {
        let (svc, tx, handle, _d) = running_service();
        svc.set_dpi_levels("dev-1", vec![400, 3200], 1).unwrap();
        assert_eq!(svc.get_device_state("dev-1").unwrap().dpi_current, 3200);
        svc.set_active_dpi_level("dev-1", 0).unwrap();
        svc.set_polling_rate("dev-1", 500).unwrap();

        let invalid = |r: fdo::Result<()>| matches!(r, Err(fdo::Error::InvalidArgs(_)));
        assert!(invalid(svc.set_dpi_levels("dev-1", vec![125], 0))); // fora da grade
        assert!(invalid(svc.set_dpi_levels("dev-1", vec![70000], 0))); // não cabe em u16
        assert!(invalid(svc.set_dpi_levels("dev-1", vec![], 0)));
        assert!(invalid(svc.set_dpi_levels("dev-1", vec![400], 9)));
        assert!(invalid(svc.set_active_dpi_level("dev-1", 7)));
        assert!(invalid(svc.set_polling_rate("dev-1", 333)));
        assert!(invalid(svc.set_polling_rate("dev-1", 70000)));
        assert!(invalid(svc.create_profile("")));
        assert!(invalid(svc.create_profile("a\nb")));
        // Nada disso mexeu no mouse.
        assert_eq!(svc.get_device_state("dev-1").unwrap().dpi_current, 400);
        stop(tx, handle);
    }

    #[test]
    fn unknown_ids_and_profiles_map_to_unknown_object() {
        let (svc, tx, handle, _d) = running_service();
        assert!(matches!(svc.get_device_state("nope"), Err(fdo::Error::UnknownObject(_))));
        assert!(matches!(svc.activate_profile("zzz"), Err(fdo::Error::UnknownObject(_))));
        assert!(matches!(svc.delete_profile("zzz"), Err(fdo::Error::UnknownObject(_))));
        stop(tx, handle);
    }

    #[test]
    fn profiles_can_be_created_activated_and_deleted() {
        let (svc, tx, handle, _d) = running_service();
        svc.create_profile("Trabalho").unwrap();
        svc.activate_profile("Trabalho").unwrap();
        assert_eq!(svc.get_active_profile().unwrap(), "Trabalho");
        svc.activate_profile("Padrão").unwrap();
        svc.delete_profile("Trabalho").unwrap();
        svc.rescan().unwrap();
        stop(tx, handle);
    }

    #[test]
    fn stopped_manager_gives_failed_not_a_hang() {
        let (svc, tx, handle, _d) = running_service();
        stop(tx, handle);
        assert!(matches!(svc.devices(), Err(fdo::Error::Failed(_))));
    }

    #[test]
    fn slow_manager_times_out_with_failed() {
        // Ninguém atende o canal: o serviço deve desistir em vez de travar o cliente.
        let (tx, _rx_kept_alive) = channel::<Command>();
        let svc = DaemonService::with_timeout(tx, Duration::from_millis(50));
        assert!(matches!(svc.devices(), Err(fdo::Error::Failed(_))));
    }

    #[test]
    fn led_roundtrip_and_validation_over_the_service() {
        let (svc, tx, handle, _d) = running_service();
        assert_eq!(svc.led_effects("dev-1").unwrap(), vec!["off", "fixed", "cycle", "breathing"]);
        let cur = svc.get_led("dev-1").unwrap();
        assert_eq!((cur.effect.as_str(), cur.period_ms), ("cycle", 8000));
        svc.set_led("dev-1", "fixed", "#FE8019", 0, 0).unwrap();
        let cur = svc.get_led("dev-1").unwrap();
        assert_eq!((cur.effect.as_str(), cur.color.as_str()), ("fixed", "#FE8019"));

        let invalid = |r: fdo::Result<()>| matches!(r, Err(fdo::Error::InvalidArgs(_)));
        assert!(invalid(svc.set_led("dev-1", "sparkle", "", 0, 0)));
        assert!(invalid(svc.set_led("dev-1", "fixed", "#GGG", 0, 0)));
        assert!(invalid(svc.set_led("dev-1", "fixed", "", 0, 0)));
        assert!(invalid(svc.set_led("dev-1", "cycle", "", 0, 0)));
        assert!(matches!(svc.get_led("nope"), Err(fdo::Error::UnknownObject(_))));
        // Só a gravação boa chegou ao mouse.
        assert_eq!(svc.get_led("dev-1").unwrap().effect, "fixed");
        stop(tx, handle);
    }
}
