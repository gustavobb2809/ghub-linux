//! Ponte cxx-qt: expoe o estado do controller ao QML. Sem logica de negocio
//! (tudo vem de `crate::model`).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, daemon_state, cxx_name = "daemonState")]
        #[qproperty(bool, has_device, cxx_name = "hasDevice")]
        #[qproperty(QString, device_name, cxx_name = "deviceName")]
        #[qproperty(bool, connected)]
        #[qproperty(i32, battery_percent, cxx_name = "batteryPercent")]
        #[qproperty(QString, battery_text, cxx_name = "batteryText")]
        #[qproperty(bool, battery_low, cxx_name = "batteryLow")]
        #[qproperty(i32, dpi_current, cxx_name = "dpiCurrent")]
        #[qproperty(QString, dpi_text, cxx_name = "dpiText")]
        #[qproperty(QString, dpi_levels_json, cxx_name = "dpiLevelsJson")]
        #[qproperty(i32, active_level, cxx_name = "activeLevel")]
        #[qproperty(i32, dpi_min, cxx_name = "dpiMin")]
        #[qproperty(i32, dpi_max, cxx_name = "dpiMax")]
        #[qproperty(i32, dpi_step, cxx_name = "dpiStep")]
        #[qproperty(i32, polling_hz, cxx_name = "pollingHz")]
        #[qproperty(QString, polling_text, cxx_name = "pollingText")]
        #[qproperty(QString, polling_options_json, cxx_name = "pollingOptionsJson")]
        #[qproperty(QString, profiles_json, cxx_name = "profilesJson")]
        #[qproperty(QString, active_profile, cxx_name = "activeProfile")]
        #[qproperty(bool, profile_differs, cxx_name = "profileDiffers")]
        #[qproperty(QString, profile_diff_text, cxx_name = "profileDiffText")]
        #[qproperty(bool, autostart_known, cxx_name = "autostartKnown")]
        #[qproperty(bool, autostart_enabled, cxx_name = "autostartEnabled")]
        #[qproperty(QString, devices_json, cxx_name = "devicesJson")]
        #[qproperty(bool, show_device_selector, cxx_name = "showDeviceSelector")]
        #[qproperty(QString, selected_device, cxx_name = "selectedDevice")]
        #[qproperty(QString, last_error, cxx_name = "lastError")]
        type Backend = super::BackendRust;

        #[qinvokable]
        fn start(self: Pin<&mut Backend>);
        #[qinvokable]
        #[cxx_name = "setDpiLevel"]
        fn set_dpi_level(self: Pin<&mut Backend>, index: i32, dpi: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "addLevel"]
        fn add_level(self: Pin<&mut Backend>) -> QString;
        #[qinvokable]
        #[cxx_name = "removeLevel"]
        fn remove_level(self: Pin<&mut Backend>, index: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "selectLevel"]
        fn select_level(self: Pin<&mut Backend>, index: i32);
        #[qinvokable]
        #[cxx_name = "commitDpi"]
        fn commit_dpi(self: Pin<&mut Backend>);
        #[qinvokable]
        #[cxx_name = "setPolling"]
        fn set_polling(self: Pin<&mut Backend>, hz: i32);
        #[qinvokable]
        #[cxx_name = "applyProfileToDevice"]
        fn apply_profile_to_device(self: Pin<&mut Backend>);
        #[qinvokable]
        #[cxx_name = "activateProfile"]
        fn activate_profile(self: Pin<&mut Backend>, name: QString);
        #[qinvokable]
        #[cxx_name = "createProfile"]
        fn create_profile(self: Pin<&mut Backend>, name: QString) -> QString;
        #[qinvokable]
        #[cxx_name = "deleteProfile"]
        fn delete_profile(self: Pin<&mut Backend>, name: QString);
        #[qinvokable]
        #[cxx_name = "selectDevice"]
        fn select_device(self: Pin<&mut Backend>, id: QString);
        #[qinvokable]
        #[cxx_name = "setAutostart"]
        fn set_autostart(self: Pin<&mut Backend>, on: bool);
        #[qinvokable]
        #[cxx_name = "startDaemon"]
        fn start_daemon(self: Pin<&mut Backend>);
        #[qinvokable]
        fn rescan(self: Pin<&mut Backend>);
        #[qinvokable]
        #[cxx_name = "validateDpi"]
        fn validate_dpi(self: Pin<&mut Backend>, dpi: i32) -> QString;
        #[qinvokable]
        #[cxx_name = "clearError"]
        fn clear_error(self: Pin<&mut Backend>);

        #[qsignal]
        #[cxx_name = "activateRequested"]
        fn activate_requested(self: Pin<&mut Backend>);
    }

    impl cxx_qt::Threading for Backend {}
}

use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use crate::client::controller::{self, Command, Config, Msg, Update};
use crate::client::DaemonApi;
use crate::model::dpi::{self, DpiRange, Levels};
use crate::model::names;
use crate::model::props::Props;
use crate::model::state::ViewState;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;

pub enum Mode {
    Real,
    /// Sem daemon: `start()` nao faz nada (smokes que nao precisam de backend).
    Idle,
    #[cfg(feature = "fake-daemon")]
    Fake(String),
}
/// Modo global, definido pelo `main` antes de `Backend::start`.
pub static MODE: OnceLock<Mode> = OnceLock::new();

/// Receptor de ativacoes (instancia unica), entregue pelo `main`.
pub static ACTIVATIONS: OnceLock<Mutex<Option<Receiver<()>>>> = OnceLock::new();
/// Thread do controller; o `main` a junta no encerramento (flush da edicao pendente).
pub static CONTROLLER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

pub struct BackendRust {
    tx: Option<Sender<Msg>>,
    view: ViewState,
    daemon_state: QString,
    has_device: bool,
    device_name: QString,
    connected: bool,
    battery_percent: i32,
    battery_text: QString,
    battery_low: bool,
    dpi_current: i32,
    dpi_text: QString,
    dpi_levels_json: QString,
    active_level: i32,
    dpi_min: i32,
    dpi_max: i32,
    dpi_step: i32,
    polling_hz: i32,
    polling_text: QString,
    polling_options_json: QString,
    profiles_json: QString,
    active_profile: QString,
    profile_differs: bool,
    profile_diff_text: QString,
    autostart_known: bool,
    autostart_enabled: bool,
    devices_json: QString,
    show_device_selector: bool,
    selected_device: QString,
    last_error: QString,
}

impl Default for BackendRust {
    fn default() -> Self {
        let p = Props::from(&ViewState::default());
        BackendRust {
            tx: None,
            view: ViewState::default(),
            daemon_state: QString::from(p.daemon_state.as_str()),
            has_device: p.has_device,
            device_name: QString::from(p.device_name.as_str()),
            connected: p.connected,
            battery_percent: p.battery_percent,
            battery_text: QString::from(p.battery_text.as_str()),
            battery_low: p.battery_low,
            dpi_current: p.dpi_current,
            dpi_text: QString::from(p.dpi_text.as_str()),
            dpi_levels_json: QString::from(p.dpi_levels_json.as_str()),
            active_level: p.active_level,
            dpi_min: p.dpi_min,
            dpi_max: p.dpi_max,
            dpi_step: p.dpi_step,
            polling_hz: p.polling_hz,
            polling_text: QString::from(p.polling_text.as_str()),
            polling_options_json: QString::from(p.polling_options_json.as_str()),
            profiles_json: QString::from(p.profiles_json.as_str()),
            active_profile: QString::from(p.active_profile.as_str()),
            profile_differs: p.profile_differs,
            profile_diff_text: QString::from(p.profile_diff_text.as_str()),
            autostart_known: p.autostart_known,
            autostart_enabled: p.autostart_enabled,
            devices_json: QString::from(p.devices_json.as_str()),
            show_device_selector: p.show_device_selector,
            selected_device: QString::from(p.selected_device.as_str()),
            last_error: QString::default(),
        }
    }
}

impl Drop for BackendRust {
    fn drop(&mut self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Msg::Quit);
        }
    }
}

const NO_DEVICE: &str = "Nenhum dispositivo selecionado.";

impl qobject::Backend {
    fn send(&self, c: Command) {
        if let Some(tx) = &self.rust().tx {
            let _ = tx.send(Msg::Command(c));
        }
    }

    fn apply(mut self: Pin<&mut Self>, update: Update) {
        match update {
            Update::State(v) => {
                let p = Props::from(&v);
                self.as_mut().rust_mut().view = v;
        self.as_mut().set_daemon_state(QString::from(p.daemon_state.as_str()));
        self.as_mut().set_has_device(p.has_device);
        self.as_mut().set_device_name(QString::from(p.device_name.as_str()));
        self.as_mut().set_connected(p.connected);
        self.as_mut().set_battery_percent(p.battery_percent);
        self.as_mut().set_battery_text(QString::from(p.battery_text.as_str()));
        self.as_mut().set_battery_low(p.battery_low);
        self.as_mut().set_dpi_current(p.dpi_current);
        self.as_mut().set_dpi_text(QString::from(p.dpi_text.as_str()));
        self.as_mut().set_dpi_levels_json(QString::from(p.dpi_levels_json.as_str()));
        self.as_mut().set_active_level(p.active_level);
        self.as_mut().set_dpi_min(p.dpi_min);
        self.as_mut().set_dpi_max(p.dpi_max);
        self.as_mut().set_dpi_step(p.dpi_step);
        self.as_mut().set_polling_hz(p.polling_hz);
        self.as_mut().set_polling_text(QString::from(p.polling_text.as_str()));
        self.as_mut().set_polling_options_json(QString::from(p.polling_options_json.as_str()));
        self.as_mut().set_profiles_json(QString::from(p.profiles_json.as_str()));
        self.as_mut().set_active_profile(QString::from(p.active_profile.as_str()));
        self.as_mut().set_profile_differs(p.profile_differs);
        self.as_mut().set_profile_diff_text(QString::from(p.profile_diff_text.as_str()));
        self.as_mut().set_autostart_known(p.autostart_known);
        self.as_mut().set_autostart_enabled(p.autostart_enabled);
        self.as_mut().set_devices_json(QString::from(p.devices_json.as_str()));
        self.as_mut().set_show_device_selector(p.show_device_selector);
        self.as_mut().set_selected_device(QString::from(p.selected_device.as_str()));
            }
            Update::Error(t) => self.as_mut().set_last_error(QString::from(t.as_str())),
            Update::LedDone => {}
        }
    }

    fn start(mut self: Pin<&mut Self>) {
        if self.rust().tx.is_some() {
            return;
        }
        let Some(mode) = MODE.get() else { return };
        if matches!(mode, Mode::Idle) {
            return;
        }
        let (tx, rx) = channel::<Msg>();
        let api: Arc<dyn DaemonApi> = match mode {
            Mode::Real => match crate::client::real::RealDaemon::connect() {
                Ok(d) => {
                    if let Err(e) = d.spawn_signal_listener(tx.clone()) {
                        eprintln!("ghub-gui: sem sinais do daemon: {e}");
                    }
                    Arc::new(d)
                }
                Err(e) => {
                    self.as_mut().set_last_error(QString::from(format!("Sem barramento de sessão: {e}").as_str()));
                    self.as_mut().set_daemon_state(QString::from("stopped"));
                    return;
                }
            },
            Mode::Idle => return,
            #[cfg(feature = "fake-daemon")]
            Mode::Fake(s) => Arc::new(crate::client::fake::FakeDaemon::scenario(s)),
        };
        let qt = self.qt_thread();
        let sink = Box::new(move |u: Update| {
            // Objeto destruido: o envio falha e e ignorado (sem panico).
            let _ = qt.queue(move |o| o.apply(u));
        });
        let handle = controller::spawn(api, Config::default(), rx, sink);
        if let Ok(mut g) = CONTROLLER.lock() {
            *g = Some(handle);
        }
        self.as_mut().rust_mut().tx = Some(tx);

        let rx = ACTIVATIONS.get().and_then(|m| m.lock().ok().and_then(|mut g| g.take()));
        if let Some(rx) = rx {
            let qt = self.qt_thread();
            std::thread::spawn(move || {
                while rx.recv().is_ok() {
                    if qt.queue(|o| o.activate_requested()).is_err() {
                        break;
                    }
                }
            });
        }
    }

    /// Aplica `f` sobre os niveis atuais; em sucesso atualiza a UI na hora e devolve os novos niveis.
    fn edit_levels(
        mut self: Pin<&mut Self>,
        f: impl FnOnce(&Levels, &DpiRange) -> Result<Levels, String>,
    ) -> Result<Levels, String> {
        let Some(d) = self.rust().view.device.clone() else { return Err(NO_DEVICE.into()) };
        let new = f(&d.levels(), &d.range)?;
        self.as_mut().rust_mut().view.device = Some(d.with_levels(new.clone()));
        let json = crate::model::format::json_u32s(&new.levels);
        self.as_mut().set_dpi_levels_json(QString::from(json.as_str()));
        self.as_mut().set_active_level(new.active as i32);
        Ok(new)
    }

    fn send_levels(&self, r: Result<Levels, String>) -> QString {
        match r {
            Ok(l) => {
                self.send(Command::SetDpiLevels { levels: l.levels, active: l.active });
                QString::default()
            }
            Err(e) => QString::from(e.as_str()),
        }
    }

    fn set_dpi_level(mut self: Pin<&mut Self>, index: i32, dpi: i32) -> QString {
        let r = self.as_mut().edit_levels(|l, r| match usize::try_from(index) {
            Ok(i) => dpi::set_level(l, r, i, i64::from(dpi)),
            Err(_) => Err("Nível inexistente.".into()),
        });
        self.send_levels(r)
    }

    fn add_level(mut self: Pin<&mut Self>) -> QString {
        let r = self.as_mut().edit_levels(dpi::add_level);
        self.send_levels(r)
    }

    fn remove_level(mut self: Pin<&mut Self>, index: i32) -> QString {
        let r = self.as_mut().edit_levels(|l, _| match usize::try_from(index) {
            Ok(i) => dpi::remove_level(l, i),
            Err(_) => Err("Nível inexistente.".into()),
        });
        self.send_levels(r)
    }

    fn select_level(mut self: Pin<&mut Self>, index: i32) {
        let r = self.as_mut().edit_levels(|l, _| match usize::try_from(index) {
            Ok(i) => dpi::select_level(l, i),
            Err(_) => Err("Nível inexistente.".into()),
        });
        match r {
            Ok(l) => self.send(Command::SetActiveLevel(l.active)),
            Err(e) => self.set_last_error(QString::from(e.as_str())),
        }
    }

    /// Chamado ao soltar o slider: o controller grava a edicao pendente sem esperar o debounce
    /// (`Refresh` descarrega a edicao pendente antes de reler).
    fn commit_dpi(self: Pin<&mut Self>) {
        self.send(Command::Refresh);
    }

    fn set_polling(self: Pin<&mut Self>, hz: i32) {
        if let Ok(hz) = u32::try_from(hz) {
            self.send(Command::SetPolling(hz));
        }
    }

    fn apply_profile_to_device(self: Pin<&mut Self>) {
        self.send(Command::ApplyProfile);
    }

    fn activate_profile(self: Pin<&mut Self>, name: QString) {
        self.send(Command::ActivateProfile(name.to_string()));
    }

    fn create_profile(self: Pin<&mut Self>, name: QString) -> QString {
        match names::validate_profile_name(&name.to_string(), &self.rust().view.profiles) {
            Ok(n) => {
                self.send(Command::CreateProfile(n));
                QString::default()
            }
            Err(e) => QString::from(e.as_str()),
        }
    }

    fn delete_profile(self: Pin<&mut Self>, name: QString) {
        let name = name.to_string();
        let v = &self.rust().view;
        if names::can_delete(&v.profiles, &v.active_profile, &name) {
            self.send(Command::DeleteProfile(name));
        }
    }

    fn select_device(self: Pin<&mut Self>, id: QString) {
        self.send(Command::SelectDevice(id.to_string()));
    }

    fn set_autostart(self: Pin<&mut Self>, on: bool) {
        self.send(Command::SetAutostart(on));
    }

    fn start_daemon(self: Pin<&mut Self>) {
        self.send(Command::StartDaemon);
    }

    fn rescan(self: Pin<&mut Self>) {
        self.send(Command::Rescan);
    }

    fn validate_dpi(self: Pin<&mut Self>, dpi: i32) -> QString {
        match &self.rust().view.device {
            Some(d) => d.range.validate(i64::from(dpi)).err().map(|e| QString::from(e.as_str())).unwrap_or_default(),
            None => QString::default(),
        }
    }

    fn clear_error(self: Pin<&mut Self>) {
        self.set_last_error(QString::default());
    }
}
