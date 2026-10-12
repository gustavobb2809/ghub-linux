use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use ghub_ipc::{DeviceState, DeviceSummary, BATTERY_UNKNOWN, STATE_UNKNOWN};
use hidpp::features::dpi::DpiSupport;

use crate::backend::Backend;
use crate::battery_sysfs::{BatteryInfo, ChargeStatus};
use crate::mouse::{Capabilities, Mouse};
use crate::profile::{dpi_allowed, Profile, ProfileError, ProfileStore};

pub const LOW_BATTERY_PERCENT: u8 = 15;
pub const BATTERY_RECOVERED_PERCENT: u8 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    DevicesChanged,
    DeviceChanged(String),
    ProfileChanged(String),
    BatteryLow { id: String, percent: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerError {
    NotFound(String),
    Invalid(String),
    Offline(String),
    Device(String),
}

impl std::fmt::Display for ManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManagerError::NotFound(what) => write!(f, "não encontrado: {what}"),
            ManagerError::Invalid(why) => write!(f, "{why}"),
            ManagerError::Offline(id) => write!(f, "o dispositivo {id} está offline"),
            ManagerError::Device(why) => write!(f, "erro no dispositivo: {why}"),
        }
    }
}

impl std::error::Error for ManagerError {}

impl From<ProfileError> for ManagerError {
    fn from(e: ProfileError) -> Self {
        match e {
            ProfileError::NotFound(n) => ManagerError::NotFound(n),
            other => ManagerError::Invalid(other.to_string()),
        }
    }
}

pub enum Command {
    Devices(Sender<Vec<DeviceSummary>>),
    State { id: String, reply: Sender<Result<DeviceState, ManagerError>> },
    SetDpiLevels { id: String, levels: Vec<u16>, active: usize, reply: Sender<Result<(), ManagerError>> },
    SetActiveLevel { id: String, active: usize, reply: Sender<Result<(), ManagerError>> },
    SetPolling { id: String, hz: u16, reply: Sender<Result<(), ManagerError>> },
    ListProfiles(Sender<Vec<String>>),
    ActiveProfile(Sender<String>),
    ActivateProfile { name: String, reply: Sender<Result<(), ManagerError>> },
    CreateProfile { name: String, reply: Sender<Result<(), ManagerError>> },
    DeleteProfile { name: String, reply: Sender<Result<(), ManagerError>> },
    /// `None` quando o pedido vem do monitor udev (ninguém espera resposta).
    Rescan(Option<Sender<()>>),
    Shutdown,
}

struct Managed {
    id: String,
    name: String,
    /// `None` quando o dispositivo está offline.
    mouse: Option<Box<dyn Mouse>>,
    caps: Option<Capabilities>,
    battery: Option<BatteryInfo>,
    low_notified: bool,
}

pub struct Manager<B: Backend> {
    backend: B,
    store: ProfileStore,
    store_path: PathBuf,
    events: Sender<Event>,
    devices: Vec<Managed>,
}

fn battery_fields(b: &Option<BatteryInfo>) -> (i32, String) {
    match b {
        Some(b) => (i32::from(b.percent), b.status.as_str().to_string()),
        None => (BATTERY_UNKNOWN, STATE_UNKNOWN.to_string()),
    }
}

impl<B: Backend> Manager<B> {
    pub fn new(backend: B, store: ProfileStore, store_path: PathBuf, events: Sender<Event>) -> Self {
        Manager { backend, store, store_path, events, devices: Vec::new() }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    fn persist(&self) {
        if let Err(e) = self.store.save(&self.store_path) {
            eprintln!("ghubd: não foi possível salvar {}: {e}", self.store_path.display());
        }
    }

    fn find(&self, id: &str) -> Result<usize, ManagerError> {
        self.devices
            .iter()
            .position(|d| d.id == id)
            .ok_or_else(|| ManagerError::NotFound(id.to_string()))
    }

    fn online_caps(&self, idx: usize) -> Result<Capabilities, ManagerError> {
        let d = &self.devices[idx];
        match (&d.mouse, &d.caps) {
            (Some(_), Some(caps)) => Ok(caps.clone()),
            _ => Err(ManagerError::Offline(d.id.clone())),
        }
    }

    // ----- consultas -----

    pub fn devices(&self) -> Vec<DeviceSummary> {
        self.devices
            .iter()
            .map(|d| {
                let (battery_percent, battery_state) = battery_fields(&d.battery);
                DeviceSummary {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    connected: d.mouse.is_some(),
                    battery_percent,
                    battery_state,
                }
            })
            .collect()
    }

    pub fn state(&mut self, id: &str) -> Result<DeviceState, ManagerError> {
        let idx = self.find(id)?;
        let profile = self.store.active().clone();
        let d = &mut self.devices[idx];
        let (dpi_current, polling_hz) = match d.mouse.as_mut() {
            Some(m) => (
                u32::from(m.read_dpi().unwrap_or(0)),
                u32::from(m.read_polling_hz().unwrap_or(0)),
            ),
            None => (0, 0),
        };
        let (dpi_min, dpi_max, dpi_step, dpi_list, polling_supported) = match &d.caps {
            Some(c) => {
                let (min, max, step, list) = match &c.dpi {
                    DpiSupport::Range { min, max, step } => {
                        (u32::from(*min), u32::from(*max), u32::from(*step), Vec::new())
                    }
                    DpiSupport::List(values) => (
                        values.first().copied().map(u32::from).unwrap_or(0),
                        values.last().copied().map(u32::from).unwrap_or(0),
                        0,
                        values.iter().copied().map(u32::from).collect(),
                    ),
                };
                (min, max, step, list, c.polling_hz.iter().copied().map(u32::from).collect())
            }
            None => (0, 0, 0, Vec::new(), Vec::new()),
        };
        let (battery_percent, battery_state) = battery_fields(&d.battery);
        Ok(DeviceState {
            id: d.id.clone(),
            name: d.name.clone(),
            connected: d.mouse.is_some(),
            dpi_current,
            dpi_levels: profile.dpi_levels.iter().copied().map(u32::from).collect(),
            active_level: profile.active_level as u32,
            dpi_min,
            dpi_max,
            dpi_step,
            dpi_list,
            polling_hz,
            polling_supported,
            battery_percent,
            battery_state,
            profile: profile.name,
        })
    }

    pub fn list_profiles(&self) -> Vec<String> {
        self.store.names()
    }

    pub fn active_profile(&self) -> String {
        self.store.active_name().to_string()
    }

    // ----- mudanças -----

    /// Grava o perfil e o aplica. O perfil muda mesmo se a escrita no dispositivo falhar;
    /// nesse caso o erro da escrita é devolvido a quem pediu.
    fn commit_profile_change(&mut self, idx: usize, candidate: Profile) -> Result<(), ManagerError> {
        self.store.update_active(candidate);
        self.persist();
        let applied = self.apply_to(idx);
        let name = self.store.active_name().to_string();
        let id = self.devices[idx].id.clone();
        self.emit(Event::ProfileChanged(name));
        self.emit(Event::DeviceChanged(id));
        applied
    }

    pub fn set_dpi_levels(&mut self, id: &str, levels: Vec<u16>, active: usize) -> Result<(), ManagerError> {
        let idx = self.find(id)?;
        let caps = self.online_caps(idx)?;
        let mut candidate = self.store.active().clone();
        candidate.dpi_levels = levels;
        candidate.active_level = active;
        candidate.check_shape()?;
        for dpi in &candidate.dpi_levels {
            if !dpi_allowed(&caps.dpi, *dpi) {
                return Err(ManagerError::Invalid(format!(
                    "DPI {dpi} não é suportado por este dispositivo"
                )));
            }
        }
        self.commit_profile_change(idx, candidate)
    }

    pub fn set_active_level(&mut self, id: &str, active: usize) -> Result<(), ManagerError> {
        let idx = self.find(id)?;
        self.online_caps(idx)?;
        let mut candidate = self.store.active().clone();
        candidate.active_level = active;
        candidate.check_shape()?;
        self.commit_profile_change(idx, candidate)
    }

    pub fn set_polling(&mut self, id: &str, hz: u16) -> Result<(), ManagerError> {
        let idx = self.find(id)?;
        let caps = self.online_caps(idx)?;
        if !caps.polling_hz.contains(&hz) {
            return Err(ManagerError::Invalid(format!(
                "taxa de polling {hz} Hz não é suportada por este dispositivo"
            )));
        }
        let mut candidate = self.store.active().clone();
        candidate.polling_hz = hz;
        candidate.check_shape()?;
        self.commit_profile_change(idx, candidate)
    }

    pub fn activate_profile(&mut self, name: &str) -> Result<(), ManagerError> {
        self.store.activate(name)?;
        self.persist();
        for idx in 0..self.devices.len() {
            if let Err(e) = self.apply_to(idx) {
                eprintln!("ghubd: {}: falha ao aplicar o perfil {name}: {e}", self.devices[idx].id);
            }
        }
        self.emit(Event::ProfileChanged(name.to_string()));
        for id in self.devices.iter().filter(|d| d.mouse.is_some()).map(|d| d.id.clone()).collect::<Vec<_>>() {
            self.emit(Event::DeviceChanged(id));
        }
        Ok(())
    }

    pub fn create_profile(&mut self, name: &str) -> Result<(), ManagerError> {
        self.store.create(name)?;
        self.persist();
        self.emit(Event::ProfileChanged(self.store.active_name().to_string()));
        Ok(())
    }

    pub fn delete_profile(&mut self, name: &str) -> Result<(), ManagerError> {
        self.store.delete(name)?;
        self.persist();
        self.emit(Event::ProfileChanged(self.store.active_name().to_string()));
        Ok(())
    }

    /// Aplica o perfil ativo ao dispositivo, escrevendo apenas o que difere do estado atual
    /// e apenas valores que o dispositivo anunciou como suportados.
    ///
    /// Se uma escrita tentada falhar e o dispositivo não responder mais ao `ping`, ele é marcado
    /// offline e o erro é `Offline`; se ainda responder, o erro é `Device`.
    fn apply_to(&mut self, idx: usize) -> Result<(), ManagerError> {
        let profile = self.store.active().clone();
        let d = &mut self.devices[idx];
        let (Some(mouse), Some(caps)) = (d.mouse.as_mut(), d.caps.as_ref()) else {
            return Ok(());
        };
        let mut failure: Option<String> = None;
        if caps.polling_hz.contains(&profile.polling_hz) {
            if mouse.read_polling_hz().ok() != Some(profile.polling_hz) {
                if let Err(e) = mouse.set_polling_hz(profile.polling_hz) {
                    eprintln!("ghubd: falha ao definir o polling de {}: {e}", d.id);
                    failure.get_or_insert(format!("falha ao definir o polling: {e}"));
                }
            }
        } else {
            eprintln!("ghubd: {} não suporta {} Hz; ignorado", d.id, profile.polling_hz);
        }
        let dpi = profile.active_dpi();
        if dpi_allowed(&caps.dpi, dpi) {
            if mouse.read_dpi().ok() != Some(dpi) {
                if let Err(e) = mouse.set_dpi(dpi) {
                    eprintln!("ghubd: falha ao definir o DPI de {}: {e}", d.id);
                    failure.get_or_insert(format!("falha ao definir o DPI: {e}"));
                }
            }
        } else {
            eprintln!("ghubd: {} não suporta {dpi} DPI; ignorado", d.id);
        }
        let Some(why) = failure else {
            return Ok(());
        };
        if mouse.ping().is_ok() {
            return Err(ManagerError::Device(why));
        }
        d.mouse = None;
        d.caps = None;
        let id = d.id.clone();
        self.emit(Event::DeviceChanged(id.clone()));
        self.emit(Event::DevicesChanged);
        Err(ManagerError::Offline(id))
    }

    // ----- ciclo de vida dos dispositivos -----

    /// Lê as capacidades, semeia o perfil na primeira execução e aplica o perfil ativo.
    fn bring_online(&mut self, idx: usize) {
        let probe = {
            let Some(mouse) = self.devices[idx].mouse.as_mut() else {
                return;
            };
            (mouse.capabilities(), mouse.read_dpi(), mouse.read_polling_hz())
        };
        let (caps, dpi, hz) = probe;
        match caps {
            Ok(c) => self.devices[idx].caps = Some(c),
            Err(e) => {
                eprintln!("ghubd: {}: não foi possível ler as capacidades: {e}", self.devices[idx].id);
                self.devices[idx].mouse = None;
                return;
            }
        }
        if self.store.needs_seed() {
            if let (Ok(dpi), Ok(hz)) = (dpi, hz) {
                if dpi > 0 && hz > 0 {
                    self.store.seed(dpi, hz);
                    self.persist();
                }
            }
            if self.store.needs_seed() {
                // Sem os valores atuais não há como semear; escrever o perfil padrão
                // sobrescreveria a configuração do usuário. Tenta de novo no próximo `tick`.
                let d = &mut self.devices[idx];
                eprintln!("ghubd: {}: não foi possível ler DPI/polling para semear o perfil", d.id);
                d.mouse = None;
                d.caps = None;
                return;
            }
        }
        if let Err(e) = self.apply_to(idx) {
            eprintln!("ghubd: {}: falha ao aplicar o perfil: {e}", self.devices[idx].id);
        }
        let id = self.devices[idx].id.clone();
        self.emit(Event::DeviceChanged(id));
    }

    pub fn rescan(&mut self) {
        let found = self.backend.scan();
        let mut changed = false;
        for f in found {
            let idx = match self.devices.iter().position(|d| d.id == f.id) {
                Some(i) if self.devices[i].mouse.is_some() => continue,
                Some(i) => {
                    self.devices[i].name = f.name;
                    self.devices[i].mouse = Some(f.mouse);
                    i
                }
                None => {
                    self.devices.push(Managed {
                        id: f.id,
                        name: f.name,
                        mouse: Some(f.mouse),
                        caps: None,
                        battery: None,
                        low_notified: false,
                    });
                    self.devices.len() - 1
                }
            };
            self.bring_online(idx);
            changed = true;
        }
        if changed {
            self.emit(Event::DevicesChanged);
        }
    }

    fn refresh_batteries(&mut self) {
        for idx in 0..self.devices.len() {
            let id = self.devices[idx].id.clone();
            let new = self.backend.battery(&id);
            let d = &mut self.devices[idx];
            // Só porcentagem e estado contam como mudança; a tensão oscila a cada leitura.
            let key = |b: &Option<BatteryInfo>| b.as_ref().map(|b| (b.percent, b.status));
            let changed = key(&d.battery) != key(&new);
            d.battery = new.clone();
            let mut events = Vec::new();
            if let Some(b) = &new {
                let discharging = !matches!(b.status, ChargeStatus::Charging | ChargeStatus::Full);
                if b.percent <= LOW_BATTERY_PERCENT && discharging && !d.low_notified {
                    d.low_notified = true;
                    events.push(Event::BatteryLow { id: id.clone(), percent: b.percent });
                } else if b.percent >= BATTERY_RECOVERED_PERCENT || !discharging {
                    d.low_notified = false;
                }
            }
            if changed {
                events.push(Event::DeviceChanged(id));
            }
            for e in events {
                self.emit(e);
            }
        }
    }

    /// Ciclo periódico: detecta dispositivos que sumiram, procura os que voltaram e atualiza a bateria.
    pub fn tick(&mut self) {
        let mut went_offline = Vec::new();
        for d in &mut self.devices {
            let alive = match d.mouse.as_mut() {
                Some(m) => m.ping().is_ok(),
                None => continue,
            };
            if !alive {
                d.mouse = None;
                d.caps = None;
                went_offline.push(d.id.clone());
            }
        }
        if !went_offline.is_empty() {
            for id in went_offline {
                self.emit(Event::DeviceChanged(id));
            }
            self.emit(Event::DevicesChanged);
        }
        if self.devices.is_empty() || self.devices.iter().any(|d| d.mouse.is_none()) {
            self.rescan();
        }
        self.refresh_batteries();
    }

    pub fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Devices(r) => {
                let _ = r.send(self.devices());
            }
            Command::State { id, reply } => {
                let _ = reply.send(self.state(&id));
            }
            Command::SetDpiLevels { id, levels, active, reply } => {
                let _ = reply.send(self.set_dpi_levels(&id, levels, active));
            }
            Command::SetActiveLevel { id, active, reply } => {
                let _ = reply.send(self.set_active_level(&id, active));
            }
            Command::SetPolling { id, hz, reply } => {
                let _ = reply.send(self.set_polling(&id, hz));
            }
            Command::ListProfiles(r) => {
                let _ = r.send(self.list_profiles());
            }
            Command::ActiveProfile(r) => {
                let _ = r.send(self.active_profile());
            }
            Command::ActivateProfile { name, reply } => {
                let _ = reply.send(self.activate_profile(&name));
            }
            Command::CreateProfile { name, reply } => {
                let _ = reply.send(self.create_profile(&name));
            }
            Command::DeleteProfile { name, reply } => {
                let _ = reply.send(self.delete_profile(&name));
            }
            Command::Rescan(reply) => {
                self.rescan();
                if let Some(r) = reply {
                    let _ = r.send(());
                }
            }
            Command::Shutdown => {}
        }
    }

    /// Laço principal: atende comandos e executa `tick` a cada `tick`, mesmo sob tráfego contínuo de comandos.
    pub fn run(mut self, commands: Receiver<Command>, tick: Duration) {
        self.rescan();
        let mut next_tick = Instant::now() + tick;
        loop {
            let wait = next_tick.saturating_duration_since(Instant::now());
            match commands.recv_timeout(wait) {
                Ok(Command::Shutdown) => break,
                Ok(cmd) => self.handle(cmd),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            if Instant::now() >= next_tick {
                self.tick();
                next_tick = Instant::now() + tick;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeBackend, FakeMouse};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    type Handles = (
        Manager<FakeBackend>,
        Receiver<Event>,
        FakeMouse,
        Arc<Mutex<Option<BatteryInfo>>>,
        tempfile::TempDir,
    );

    fn setup_with(store: ProfileStore, dpi: u16, polling: u16) -> Handles {
        let dir = tempfile::tempdir().unwrap();
        let mouse = FakeMouse::new(dpi, polling);
        let (backend, battery) = FakeBackend::with_mouse(mouse.clone());
        let (tx, rx) = channel();
        let manager = Manager::new(backend, store, dir.path().join("profiles.toml"), tx);
        (manager, rx, mouse, battery, dir)
    }

    /// Armazenamento já "semeado": níveis 800 e 1600, ativo 1600, 1000 Hz.
    fn seeded_store() -> ProfileStore {
        ProfileStore::from_toml(
            "active = \"Padrão\"\n[[profile]]\nname = \"Padrão\"\ndpi_levels = [800, 1600]\nactive_level = 1\npolling_hz = 1000\n",
        )
        .unwrap()
    }

    fn drain(rx: &Receiver<Event>) -> Vec<Event> {
        rx.try_iter().collect()
    }

    fn battery(percent: u8, status: ChargeStatus) -> Option<BatteryInfo> {
        Some(BatteryInfo { percent, status, voltage_mv: Some(3900) })
    }

    #[test]
    fn first_run_seeds_profile_and_does_not_write_to_the_mouse() {
        let (mut m, rx, mouse, _b, dir) = setup_with(ProfileStore::default_store(), 1200, 1000);
        m.rescan();
        assert_eq!(m.store.active().dpi_levels, vec![1200]);
        assert_eq!(m.store.active().polling_hz, 1000);
        assert!(mouse.state().dpi_sets.is_empty());
        assert!(mouse.state().polling_sets.is_empty());
        let saved = std::fs::read_to_string(dir.path().join("profiles.toml")).unwrap();
        assert!(saved.contains("1200"));
        assert!(drain(&rx).contains(&Event::DevicesChanged));
    }

    #[test]
    fn rescan_applies_the_active_profile_when_it_differs() {
        let (mut m, rx, mouse, _b, _d) = setup_with(seeded_store(), 1200, 500);
        m.rescan();
        assert_eq!(mouse.state().dpi_sets, vec![1600]);
        assert_eq!(mouse.state().polling_sets, vec![1000]);
        let events = drain(&rx);
        assert!(events.contains(&Event::DeviceChanged("dev-1".into())));
        assert!(events.contains(&Event::DevicesChanged));
    }

    #[test]
    fn devices_and_state_report_what_the_device_says() {
        let (mut m, _rx, _mouse, battery_handle, _d) = setup_with(seeded_store(), 1600, 1000);
        *battery_handle.lock().unwrap() = battery(91, ChargeStatus::Discharging);
        m.rescan();
        m.tick();
        let list = m.devices();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "dev-1");
        assert_eq!(list[0].name, "G703 LS");
        assert!(list[0].connected);
        assert_eq!(list[0].battery_percent, 91);
        assert_eq!(list[0].battery_state, "discharging");
        let st = m.state("dev-1").unwrap();
        assert_eq!(st.dpi_current, 1600);
        assert_eq!(st.dpi_levels, vec![800, 1600]);
        assert_eq!(st.active_level, 1);
        assert_eq!((st.dpi_min, st.dpi_max, st.dpi_step), (100, 25600, 50));
        assert_eq!(st.polling_hz, 1000);
        assert_eq!(st.polling_supported, vec![125, 250, 500, 1000]);
        assert_eq!(st.profile, "Padrão");
    }

    #[test]
    fn unknown_device_is_not_found() {
        let (mut m, _rx, _mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        assert!(matches!(m.state("nope"), Err(ManagerError::NotFound(_))));
        assert!(matches!(
            m.set_active_level("nope", 0),
            Err(ManagerError::NotFound(_))
        ));
    }

    #[test]
    fn set_dpi_levels_validates_before_touching_the_device() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        let before = mouse.state().dpi_sets.len();
        // 125 não está na grade de 50 a partir de 100; lista vazia; excesso; nível ativo fora.
        for (levels, active) in [
            (vec![125u16], 0usize),
            (vec![], 0),
            (vec![200, 300, 400, 500, 600, 700], 0),
            (vec![400, 800], 2),
            (vec![50], 0),
        ] {
            assert!(
                matches!(m.set_dpi_levels("dev-1", levels.clone(), active), Err(ManagerError::Invalid(_))),
                "{levels:?}/{active}"
            );
        }
        assert_eq!(mouse.state().dpi_sets.len(), before);
        assert_eq!(m.store.active().dpi_levels, vec![800, 1600]);
    }

    #[test]
    fn set_dpi_levels_applies_persists_and_notifies() {
        let (mut m, rx, mouse, _b, dir) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        drain(&rx);
        m.set_dpi_levels("dev-1", vec![400, 3200], 1).unwrap();
        assert_eq!(mouse.state().dpi, 3200);
        assert_eq!(m.store.active().dpi_levels, vec![400, 3200]);
        let saved = std::fs::read_to_string(dir.path().join("profiles.toml")).unwrap();
        assert!(saved.contains("3200"));
        let events = drain(&rx);
        assert!(events.contains(&Event::ProfileChanged("Padrão".into())));
        assert!(events.contains(&Event::DeviceChanged("dev-1".into())));
    }

    #[test]
    fn set_active_level_switches_the_dpi() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        m.set_active_level("dev-1", 0).unwrap();
        assert_eq!(mouse.state().dpi, 800);
        assert!(matches!(
            m.set_active_level("dev-1", 5),
            Err(ManagerError::Invalid(_))
        ));
    }

    #[test]
    fn set_polling_validates_against_the_device() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        assert!(matches!(m.set_polling("dev-1", 333), Err(ManagerError::Invalid(_))));
        assert!(matches!(m.set_polling("dev-1", 0), Err(ManagerError::Invalid(_))));
        m.set_polling("dev-1", 500).unwrap();
        assert_eq!(mouse.state().polling, 500);
        assert_eq!(m.store.active().polling_hz, 500);
    }

    #[test]
    fn offline_device_rejects_writes() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        mouse.state().online = false;
        m.tick();
        assert!(matches!(
            m.set_dpi_levels("dev-1", vec![800], 0),
            Err(ManagerError::Offline(_))
        ));
        let st = m.state("dev-1").unwrap();
        assert!(!st.connected);
        assert_eq!(st.dpi_current, 0);
        assert_eq!(st.dpi_levels, vec![800, 1600]);
    }

    #[test]
    fn reconnect_reapplies_the_profile() {
        let (mut m, rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        drain(&rx);
        mouse.state().online = false;
        m.tick();
        assert!(!m.devices()[0].connected);
        assert!(drain(&rx).contains(&Event::DeviceChanged("dev-1".into())));
        // O mouse volta com as configurações de fábrica.
        {
            let mut s = mouse.state();
            s.online = true;
            s.dpi = 800;
            s.polling = 125;
        }
        m.tick();
        assert!(m.devices()[0].connected);
        assert_eq!(mouse.state().dpi, 1600);
        assert_eq!(mouse.state().polling, 1000);
    }

    #[test]
    fn battery_low_is_reported_once_until_recovery() {
        let (mut m, rx, _mouse, bat, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        drain(&rx);
        let lows = |rx: &Receiver<Event>| {
            drain(rx)
                .into_iter()
                .filter(|e| matches!(e, Event::BatteryLow { .. }))
                .count()
        };
        *bat.lock().unwrap() = battery(14, ChargeStatus::Discharging);
        m.tick();
        assert_eq!(lows(&rx), 1);
        m.tick();
        assert_eq!(lows(&rx), 0);
        *bat.lock().unwrap() = battery(25, ChargeStatus::Discharging);
        m.tick();
        assert_eq!(lows(&rx), 0);
        *bat.lock().unwrap() = battery(10, ChargeStatus::Discharging);
        m.tick();
        assert_eq!(lows(&rx), 1);
    }

    #[test]
    fn charging_battery_never_reports_low() {
        let (mut m, rx, _mouse, bat, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        drain(&rx);
        *bat.lock().unwrap() = battery(5, ChargeStatus::Charging);
        m.tick();
        assert!(!drain(&rx).iter().any(|e| matches!(e, Event::BatteryLow { .. })));
    }

    #[test]
    fn profile_lifecycle_through_the_manager() {
        let (mut m, rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        m.create_profile("Trabalho").unwrap();
        assert!(matches!(m.create_profile("Trabalho"), Err(ManagerError::Invalid(_))));
        assert_eq!(m.list_profiles(), vec!["Padrão".to_string(), "Trabalho".to_string()]);
        m.set_dpi_levels("dev-1", vec![400, 800], 0).unwrap(); // altera o ativo (Padrão)
        drain(&rx);
        m.activate_profile("Trabalho").unwrap(); // cópia do Padrão no momento da criação: 800/1600, ativo 1600
        assert_eq!(m.active_profile(), "Trabalho");
        assert_eq!(mouse.state().dpi, 1600);
        assert!(drain(&rx).contains(&Event::ProfileChanged("Trabalho".into())));
        assert!(matches!(m.activate_profile("zzz"), Err(ManagerError::NotFound(_))));
        assert!(matches!(m.delete_profile("Trabalho"), Err(ManagerError::Invalid(_))));
        m.delete_profile("Padrão").unwrap();
    }

    #[test]
    fn handle_dispatches_commands_to_their_reply_channels() {
        let (mut m, _rx, _mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        let (tx, rx) = channel();
        m.handle(Command::Rescan(Some(tx)));
        rx.recv().unwrap();
        let (tx, rx) = channel();
        m.handle(Command::Devices(tx));
        assert_eq!(rx.recv().unwrap().len(), 1);
        let (tx, rx) = channel();
        m.handle(Command::ActiveProfile(tx));
        assert_eq!(rx.recv().unwrap(), "Padrão");
        let (tx, rx) = channel();
        m.handle(Command::State { id: "nope".into(), reply: tx });
        assert!(matches!(rx.recv().unwrap(), Err(ManagerError::NotFound(_))));
    }

    #[test]
    fn run_stops_on_shutdown() {
        let (m, _rx, _mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        let (tx, crx) = channel();
        tx.send(Command::Shutdown).unwrap();
        m.run(crx, Duration::from_millis(10)); // retorna em vez de travar
    }

    #[test]
    fn first_run_with_failed_reads_does_not_write_and_retries() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(ProfileStore::default_store(), 1200, 1000);
        mouse.state().fail_reads = true;
        m.rescan();
        assert!(mouse.state().dpi_sets.is_empty());
        assert!(mouse.state().polling_sets.is_empty());
        assert!(!m.devices()[0].connected);
        mouse.state().fail_reads = false;
        m.tick();
        assert_eq!(m.store.active().dpi_levels, vec![1200]);
        assert_eq!(m.store.active().polling_hz, 1000);
        assert!(m.devices()[0].connected);
        assert!(mouse.state().dpi_sets.is_empty());
        assert!(mouse.state().polling_sets.is_empty());
    }

    #[test]
    fn write_failure_on_a_live_device_is_a_device_error() {
        let (mut m, _rx, mouse, _b, dir) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        mouse.state().fail_writes = true;
        assert!(matches!(
            m.set_dpi_levels("dev-1", vec![400, 3200], 1),
            Err(ManagerError::Device(_))
        ));
        assert_eq!(m.store.active().dpi_levels, vec![400, 3200]);
        let saved = std::fs::read_to_string(dir.path().join("profiles.toml")).unwrap();
        assert!(saved.contains("3200"));
        assert!(m.devices()[0].connected);
    }

    #[test]
    fn write_failure_on_a_dead_device_marks_it_offline() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        {
            let mut s = mouse.state();
            s.fail_writes = true;
            s.fail_ping = true;
        }
        assert!(matches!(
            m.set_dpi_levels("dev-1", vec![400, 3200], 1),
            Err(ManagerError::Offline(_))
        ));
        assert!(!m.devices()[0].connected);
    }

    #[test]
    fn run_ticks_even_under_command_traffic() {
        let (m, events, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        let (ctx, crx) = channel();
        let worker = std::thread::spawn(move || m.run(crx, Duration::from_millis(50)));
        // Espera o rescan inicial.
        loop {
            match events.recv_timeout(Duration::from_secs(5)).unwrap() {
                Event::DevicesChanged => break,
                _ => continue,
            }
        }
        drain(&events);
        mouse.state().online = false;
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(400) {
            let (tx, rx) = channel();
            ctx.send(Command::ListProfiles(tx)).unwrap();
            rx.recv().unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        ctx.send(Command::Shutdown).unwrap();
        worker.join().unwrap();
        assert!(drain(&events).contains(&Event::DevicesChanged));
    }

    #[test]
    fn battery_voltage_jitter_is_not_a_change() {
        let (mut m, rx, _mouse, bat, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        *bat.lock().unwrap() = battery(80, ChargeStatus::Discharging);
        m.tick();
        drain(&rx);
        *bat.lock().unwrap() =
            Some(BatteryInfo { percent: 80, status: ChargeStatus::Discharging, voltage_mv: Some(3895) });
        m.tick();
        assert!(!drain(&rx).contains(&Event::DeviceChanged("dev-1".into())));
        *bat.lock().unwrap() =
            Some(BatteryInfo { percent: 79, status: ChargeStatus::Discharging, voltage_mv: Some(3895) });
        m.tick();
        let changed = drain(&rx)
            .into_iter()
            .filter(|e| *e == Event::DeviceChanged("dev-1".into()))
            .count();
        assert_eq!(changed, 1);
    }
}
