use super::{ApiError, DaemonApi};
use crate::model::debounce::Debouncer;
use crate::model::state::*;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    SetDpiLevels { levels: Vec<u32>, active: u32 },
    SetActiveLevel(u32),
    SetPolling(u32),
    ApplyProfile,
    ActivateProfile(String),
    CreateProfile(String),
    DeleteProfile(String),
    SelectDevice(String),
    SetAutostart(bool),
    StartDaemon,
    Rescan,
    Refresh,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event { DevicesChanged, DeviceChanged(String), ProfileChanged(String), OwnerChanged(bool) }
#[derive(Debug)]
pub enum Msg { Command(Command), Event(Event), Quit }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Update { State(ViewState), Error(String) }

#[derive(Debug, Clone, Copy)]
pub struct Config { pub debounce: Duration, pub coalesce: Duration }
impl Default for Config {
    fn default() -> Self { Config { debounce: Duration::from_millis(150), coalesce: Duration::from_millis(100) } }
}

pub fn snapshot(api: &dyn DaemonApi, selected: Option<&str>, autostart: Option<bool>) -> ViewState {
    let mut v = ViewState { autostart, ..ViewState::default() };
    if !api.daemon_running() { v.status = DaemonStatus::Stopped; return v; }
    v.status = DaemonStatus::Running;
    let summaries = match api.devices() {
        Ok(s) => s,
        Err(ApiError::Unavailable) => { v.status = DaemonStatus::Stopped; return v; }
        Err(e) => { v.read_error = Some(e.to_string()); vec![] }
    };
    v.choices = summaries.iter().filter(|d| d.connected)
        .map(|d| DeviceChoice { id: d.id.clone(), name: d.name.clone() }).collect();
    if let Some(s) = pick_device(&summaries, selected) {
        match api.get_device_state(&s.id) {
            Ok(st) => v.device = Some(DeviceView::from_state(&st)),
            Err(e) => v.read_error = Some(e.to_string()),
        }
    }
    v.profiles = api.list_profiles().unwrap_or_default();
    v.active_profile = api.get_active_profile().unwrap_or_default();
    v
}

struct Controller {
    api: Arc<dyn DaemonApi>,
    cfg: Config,
    sink: Box<dyn Fn(Update) + Send>,
    selected: Option<String>,
    autostart: Option<bool>,
    last: ViewState,
    pending: Debouncer<(Vec<u32>, u32)>,
    refresh_at: Option<Instant>,
}

pub fn spawn(api: Arc<dyn DaemonApi>, cfg: Config, rx: Receiver<Msg>, sink: Box<dyn Fn(Update) + Send>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut c = Controller {
            autostart: api.autostart(), api, cfg, sink, selected: None,
            last: ViewState::default(), pending: Debouncer::new(cfg.debounce), refresh_at: None,
        };
        c.refresh();
        loop {
            let deadline = [c.pending.next_deadline(), c.refresh_at].into_iter().flatten().min();
            let msg = match deadline {
                Some(d) => match rx.recv_timeout(d.saturating_duration_since(Instant::now())) {
                    Ok(m) => Some(m),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                },
                None => match rx.recv() { Ok(m) => Some(m), Err(_) => break },
            };
            match msg {
                Some(Msg::Quit) => break,
                Some(Msg::Command(cmd)) => c.command(cmd),
                Some(Msg::Event(_)) => { c.refresh_at.get_or_insert(Instant::now() + c.cfg.coalesce); }
                None => {}
            }
            let now = Instant::now();
            if let Some(p) = c.pending.take_due(now) { c.write_levels(p); c.refresh(); }
            if c.refresh_at.is_some_and(|t| t <= now) { c.refresh_at = None; c.refresh(); }
        }
        if let Some(p) = c.pending.flush() { c.write_levels(p); }
    })
}

impl Controller {
    fn refresh(&mut self) {
        let mut v = snapshot(&*self.api, self.selected.as_deref(), self.autostart);
        if let Some((levels, active)) = self.pending.peek() {
            let over = crate::model::dpi::Levels { levels: levels.clone(), active: *active };
            v.device = v.device.take().map(|d| d.with_levels(over));
        }
        // Mantém a seleção durante daemon parado / erro de leitura transitório; só limpa
        // quando o daemon responde e o dispositivo realmente não existe mais.
        match &v.device {
            Some(d) => self.selected = Some(d.id.clone()),
            None if v.status == DaemonStatus::Running && v.read_error.is_none() => self.selected = None,
            None => {}
        }
        if let Some(e) = v.read_error.clone() { (self.sink)(Update::Error(e)); }
        self.last = v.clone();
        (self.sink)(Update::State(v));
    }

    fn device_id(&self) -> Option<String> { self.last.device.as_ref().map(|d| d.id.clone()) }
    fn fail(&self, text: impl Into<String>) { (self.sink)(Update::Error(text.into())); }
    fn report(&self, r: Result<(), ApiError>) { if let Err(e) = r { self.fail(e.to_string()); } }
    /// O último estado visto era "daemon parado": escritas viram erro na hora, sem tocar na API.
    fn is_stopped(&self) -> bool { self.last.status == DaemonStatus::Stopped }
    /// Confirma de verdade que o daemon está fora: se o último estado era Stopped, relê antes
    /// de decidir (o sinal de volta do daemon pode ainda estar na janela de coalescência).
    fn daemon_down(&mut self) -> bool {
        if !self.is_stopped() { return false; }
        self.refresh();
        self.is_stopped()
    }

    fn write_levels(&mut self, (levels, active): (Vec<u32>, u32)) {
        if self.daemon_down() {
            self.fail(format!("A edição de DPI não foi aplicada: {}", ApiError::Unavailable));
            return;
        }
        match self.device_id() {
            Some(id) => { let r = self.api.set_dpi_levels(&id, levels, active); self.report(r); }
            None => self.fail("Nenhum dispositivo selecionado."),
        }
    }

    fn command(&mut self, cmd: Command) {
        if let Command::SetDpiLevels { levels, active } = cmd {
            self.pending.push((levels, active), Instant::now());
            return;
        }
        let needs_daemon = !matches!(cmd,
            Command::SelectDevice(_) | Command::SetAutostart(_) | Command::StartDaemon | Command::Refresh);
        if needs_daemon && self.daemon_down() {
            // Daemon realmente fora (a releitura acima confirmou): falha imediata, sem tocar na API.
            if self.pending.flush().is_some() {
                self.fail(format!("A edição de DPI não foi aplicada: {}", ApiError::Unavailable));
            }
            self.fail(ApiError::Unavailable.to_string());
            return;
        }
        // Mantém a ordem: a edição pendente do slider vai antes de qualquer outro comando.
        if let Some(p) = self.pending.flush() { self.write_levels(p); }
        match cmd {
            Command::SetDpiLevels { .. } => unreachable!(),
            Command::SetActiveLevel(i) => self.with_device(|c, id| c.api.set_active_dpi_level(&id, i)),
            Command::SetPolling(hz) => self.with_device(|c, id| c.api.set_polling_rate(&id, hz)),
            Command::ApplyProfile => {
                let level = self.last.device.as_ref().map(|d| d.active_level);
                match level { Some(l) => self.with_device(|c, id| c.api.set_active_dpi_level(&id, l)),
                              None => self.fail("Nenhum dispositivo selecionado.") }
            }
            Command::ActivateProfile(n) => { let r = self.api.activate_profile(&n); self.report(r); }
            Command::CreateProfile(n) => { let r = self.api.create_profile(&n); self.report(r); }
            Command::DeleteProfile(n) => { let r = self.api.delete_profile(&n); self.report(r); }
            Command::SelectDevice(id) => self.selected = Some(id),
            Command::SetAutostart(on) => {
                if let Err(e) = self.api.set_autostart(on) { self.fail(e); }
                self.autostart = self.api.autostart();
            }
            Command::StartDaemon => { if let Err(e) = self.api.start_daemon() { self.fail(e); } }
            Command::Rescan => { let r = self.api.rescan(); self.report(r); }
            Command::Refresh => {}
        }
        self.refresh();
    }

    fn with_device(&self, f: impl FnOnce(&Self, String) -> Result<(), ApiError>) {
        match self.device_id() {
            Some(id) => { let r = f(self, id); self.report(r); }
            None => self.fail("Nenhum dispositivo selecionado."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::fake::{sample_state, FakeDaemon};
    use crate::model::props::Props;
    use std::sync::mpsc::{channel, Sender};

    type Rig = (Sender<Msg>, Receiver<Update>, JoinHandle<()>);

    fn rig_cfg(api: Arc<dyn DaemonApi>, cfg: Config) -> Rig {
        let (tx, rx) = channel();
        let (utx, urx) = channel();
        let h = spawn(api, cfg, rx, Box::new(move |u| { let _ = utx.send(u); }));
        (tx, urx, h)
    }
    fn rig(daemon: Arc<FakeDaemon>) -> Rig {
        rig_cfg(daemon, Config { debounce: Duration::from_millis(40), coalesce: Duration::from_millis(20) })
    }
    /// Debounce longo: rajadas e janelas de debounce ficam longe de qualquer escalonamento atrasado.
    fn rig_slow_debounce(daemon: Arc<FakeDaemon>) -> Rig {
        rig_cfg(daemon, Config { debounce: Duration::from_millis(250), coalesce: Duration::from_millis(20) })
    }
    fn next_state(urx: &Receiver<Update>) -> ViewState {
        loop {
            match urx.recv_timeout(Duration::from_secs(2)).expect("sem estado em 2 s") {
                Update::State(s) => return s,
                Update::Error(_) => {}
            }
        }
    }
    fn next_error(urx: &Receiver<Update>) -> String {
        loop {
            match urx.recv_timeout(Duration::from_secs(2)).expect("sem erro em 2 s") {
                Update::Error(e) => return e,
                Update::State(_) => {}
            }
        }
    }
    /// Drena até ficar `quiet` sem novidades; devolve tudo o que chegou.
    fn drain(urx: &Receiver<Update>, quiet: Duration) -> Vec<Update> {
        let mut all = vec![];
        while let Ok(u) = urx.recv_timeout(quiet) { all.push(u); }
        all
    }
    fn last_state_after(urx: &Receiver<Update>, quiet: Duration) -> ViewState {
        drain(urx, quiet).into_iter().rev()
            .find_map(|u| if let Update::State(s) = u { Some(s) } else { None })
            .expect("nenhum estado")
    }
    fn wait_for_call(d: &FakeDaemon, prefix: &str) {
        let end = Instant::now() + Duration::from_secs(3);
        while !d.calls().iter().any(|c| c.starts_with(prefix)) {
            assert!(Instant::now() < end, "chamada {prefix} não aconteceu");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn count(d: &FakeDaemon, prefix: &str) -> usize { d.calls().iter().filter(|c| c.starts_with(prefix)).count() }
    fn finish(tx: Sender<Msg>, h: JoinHandle<()>) { tx.send(Msg::Quit).unwrap(); h.join().unwrap(); }
    fn cmd(tx: &Sender<Msg>, c: Command) { tx.send(Msg::Command(c)).unwrap(); }
    fn second_device() -> ghub_ipc::DeviceState {
        let mut s = sample_state(); s.id = "dev-2".into(); s.name = "Outro".into(); s
    }

    #[test]
    fn first_snapshot_is_running_with_the_device() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig(d);
        let s = next_state(&urx);
        assert_eq!(s.status, DaemonStatus::Running);
        assert_eq!(s.device.as_ref().unwrap().name, "G703 LS");
        assert_eq!(s.profiles, vec!["Padrão".to_string()]);
        finish(tx, h);
    }

    #[test]
    fn daemon_stopped_yields_stopped_state_without_hanging() {
        let d = Arc::new(FakeDaemon::scenario("stopped"));
        let (tx, urx, h) = rig(d.clone());
        let s = next_state(&urx);
        assert_eq!(s.status, DaemonStatus::Stopped);
        assert!(s.device.is_none());
        cmd(&tx, Command::SetPolling(500));
        let e = next_error(&urx);
        assert!(e.contains("Nenhum dispositivo selecionado.") || e.contains("não está rodando"), "{e}");
        assert_eq!(count(&d, "set_polling_rate"), 0);
        finish(tx, h);
    }

    #[test]
    fn start_daemon_recovers() {
        let d = Arc::new(FakeDaemon::scenario("stopped"));
        let (tx, urx, h) = rig(d);
        assert_eq!(next_state(&urx).status, DaemonStatus::Stopped);
        cmd(&tx, Command::StartDaemon);
        let s = next_state(&urx);
        assert_eq!(s.status, DaemonStatus::Running);
        assert!(s.device.is_some());
        finish(tx, h);
    }

    #[test]
    fn slider_burst_writes_once_with_the_last_value() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_slow_debounce(d.clone());
        next_state(&urx);
        for i in 0..20u32 {
            cmd(&tx, Command::SetDpiLevels { levels: vec![800 + i * 50, 1200, 1600], active: 1 });
        }
        wait_for_call(&d, "set_dpi_levels");
        std::thread::sleep(Duration::from_millis(400));
        let writes: Vec<String> = d.calls().into_iter().filter(|c| c.starts_with("set_dpi_levels")).collect();
        assert_eq!(writes, vec!["set_dpi_levels dev-1 [1750, 1200, 1600] 1".to_string()]);
        finish(tx, h);
    }

    #[test]
    fn pending_edit_flushes_before_other_commands() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_slow_debounce(d.clone());
        next_state(&urx);
        cmd(&tx, Command::SetDpiLevels { levels: vec![900, 1200, 1600], active: 1 });
        cmd(&tx, Command::SetPolling(500));
        wait_for_call(&d, "set_polling_rate");
        let calls = d.calls();
        let a = calls.iter().position(|c| c.starts_with("set_dpi_levels")).expect("sem set_dpi_levels");
        let b = calls.iter().position(|c| c.starts_with("set_polling_rate")).unwrap();
        assert!(a < b, "{calls:?}");
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(count(&d, "set_dpi_levels"), 1, "a edição não pode ser escrita de novo");
        finish(tx, h);
    }

    #[test]
    fn refresh_during_debounce_does_not_revert_the_edit() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_slow_debounce(d.clone());
        next_state(&urx);
        cmd(&tx, Command::SetDpiLevels { levels: vec![900, 1200, 1600], active: 1 });
        tx.send(Msg::Event(Event::DevicesChanged)).unwrap();
        let s = next_state(&urx);
        assert_eq!(s.device.unwrap().dpi_levels, vec![900, 1200, 1600]);
        // a leitura ocorreu antes da escrita (debounce de 250 ms >> coalesce de 20 ms)
        assert_eq!(count(&d, "set_dpi_levels"), 0);
        assert_eq!(d.inner.lock().unwrap().devices[0].dpi_levels, vec![800, 1200, 1600]);
        // e depois da escrita o estado continua com o valor do usuário
        let fin = last_state_after(&urx, Duration::from_millis(500));
        assert_eq!(fin.device.unwrap().dpi_levels, vec![900, 1200, 1600]);
        assert_eq!(count(&d, "set_dpi_levels"), 1);
        finish(tx, h);
    }

    #[test]
    fn refresh_event_burst_during_a_slider_burst_never_shows_stale_levels() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_slow_debounce(d.clone());
        next_state(&urx);
        for i in 0..10u32 {
            cmd(&tx, Command::SetDpiLevels { levels: vec![900 + i * 50, 1200, 1600], active: 1 });
            tx.send(Msg::Event(Event::DeviceChanged("dev-1".into()))).unwrap();
        }
        for u in drain(&urx, Duration::from_millis(500)) {
            if let Update::State(s) = u {
                assert_ne!(s.device.unwrap().dpi_levels[0], 800, "estado antigo reapareceu");
            }
        }
        assert_eq!(count(&d, "set_dpi_levels"), 1);
        finish(tx, h);
    }

    #[test]
    fn events_are_coalesced() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_slow_debounce(d);
        next_state(&urx);
        for _ in 0..10 { tx.send(Msg::Event(Event::DeviceChanged("dev-1".into()))).unwrap(); }
        let n = drain(&urx, Duration::from_millis(300)).iter().filter(|u| matches!(u, Update::State(_))).count();
        assert!((1..=2).contains(&n), "estados: {n}");
        finish(tx, h);
    }

    #[test]
    fn write_error_is_reported_and_state_reread() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig(d.clone());
        next_state(&urx);
        d.inner.lock().unwrap().fail_next = Some(ApiError::Failed("boom".into()));
        cmd(&tx, Command::SetPolling(500));
        assert_eq!(next_error(&urx), "boom");
        assert_eq!(next_state(&urx).device.unwrap().polling_hz, 1000);
        finish(tx, h);
    }

    #[test]
    fn apply_profile_sets_the_profile_level() {
        let d = Arc::new(FakeDaemon::new());
        d.inner.lock().unwrap().devices[0].dpi_current = 1600;
        let (tx, urx, h) = rig(d.clone());
        assert!(Props::from(&next_state(&urx)).profile_differs);
        cmd(&tx, Command::ApplyProfile);
        let s = next_state(&urx);
        assert!(d.calls().contains(&"set_active_dpi_level dev-1 1".to_string()));
        assert!(!Props::from(&s).profile_differs);
        finish(tx, h);
    }

    #[test]
    fn sleeping_device_is_kept_dimmed() {
        let d = Arc::new(FakeDaemon::scenario("sleeping"));
        let (tx, urx, h) = rig(d);
        let dev = next_state(&urx).device.expect("dispositivo sumiu");
        assert!(!dev.connected);
        finish(tx, h);
    }

    #[test]
    fn refresh_keeps_the_device_when_nothing_is_pending() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig(d);
        assert!(next_state(&urx).device.is_some());
        for _ in 0..3 {
            cmd(&tx, Command::Refresh);
            assert!(next_state(&urx).device.is_some());
        }
        finish(tx, h);
    }

    #[test]
    fn quit_flushes_pending_edit() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_slow_debounce(d.clone());
        next_state(&urx);
        cmd(&tx, Command::SetDpiLevels { levels: vec![900, 1200, 1600], active: 1 });
        tx.send(Msg::Quit).unwrap();
        h.join().unwrap();
        assert_eq!(d.calls().iter().filter(|c| c.as_str() == "set_dpi_levels dev-1 [900, 1200, 1600] 1").count(), 1);
    }

    #[test]
    fn select_device_switches_view() {
        let d = Arc::new(FakeDaemon::new());
        d.inner.lock().unwrap().devices.push(second_device());
        let (tx, urx, h) = rig(d);
        let s = next_state(&urx);
        assert_eq!(s.choices.len(), 2);
        assert_eq!(s.device.unwrap().id, "dev-1");
        cmd(&tx, Command::SelectDevice("dev-2".into()));
        assert_eq!(next_state(&urx).device.unwrap().id, "dev-2");
        finish(tx, h);
    }

    // ---- daemon some / volta / lento no meio da sessão ----

    #[test]
    fn daemon_dies_mid_session_then_command_fails_immediately_and_recovers_by_itself() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig(d.clone());
        assert_eq!(next_state(&urx).status, DaemonStatus::Running);

        d.inner.lock().unwrap().running = false;
        tx.send(Msg::Event(Event::OwnerChanged(false))).unwrap();
        let s = next_state(&urx);
        assert_eq!(s.status, DaemonStatus::Stopped);

        let t0 = Instant::now();
        cmd(&tx, Command::SetPolling(500));
        let e = next_error(&urx);
        assert!(t0.elapsed() < Duration::from_millis(500), "o comando esperou");
        assert!(e.contains("não está rodando"), "{e}");
        assert_eq!(count(&d, "set_polling_rate"), 0, "não deve nem tentar a escrita");
        for c in [Command::ActivateProfile("x".into()), Command::CreateProfile("y".into()),
                  Command::DeleteProfile("z".into()), Command::Rescan, Command::ApplyProfile,
                  Command::SetActiveLevel(0)] {
            cmd(&tx, c);
            assert!(next_error(&urx).contains("não está rodando"));
        }

        d.inner.lock().unwrap().running = true;
        tx.send(Msg::Event(Event::OwnerChanged(true))).unwrap();
        let back = loop {
            let s = next_state(&urx);
            if s.status == DaemonStatus::Running { break s; }
        };
        assert!(back.device.is_some());
        // e voltou a aceitar escritas
        cmd(&tx, Command::SetPolling(500));
        assert_eq!(next_state(&urx).device.unwrap().polling_hz, 500);
        finish(tx, h);
    }

    #[test]
    fn command_after_daemon_returned_succeeds_without_waiting_for_the_signal() {
        // O sinal OwnerChanged(true) ainda não chegou: o comando relê antes de falhar e então funciona.
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig(d.clone());
        next_state(&urx);
        d.inner.lock().unwrap().running = false;
        cmd(&tx, Command::Refresh);
        assert_eq!(next_state(&urx).status, DaemonStatus::Stopped);
        d.inner.lock().unwrap().running = true;
        cmd(&tx, Command::SetPolling(500));
        let all = drain(&urx, Duration::from_millis(150));
        assert!(!all.iter().any(|u| matches!(u, Update::Error(_))), "{all:?}");
        let last = all.into_iter().rev().find_map(|u| if let Update::State(s) = u { Some(s) } else { None }).unwrap();
        assert_eq!(last.device.unwrap().polling_hz, 500);
        assert_eq!(count(&d, "set_polling_rate"), 1);
        finish(tx, h);
    }

    #[test]
    fn pending_edit_survives_a_stale_stopped_state_when_the_daemon_is_back() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_slow_debounce(d.clone());
        next_state(&urx);
        d.inner.lock().unwrap().running = false;
        cmd(&tx, Command::Refresh);
        assert_eq!(next_state(&urx).status, DaemonStatus::Stopped);
        d.inner.lock().unwrap().running = true;
        cmd(&tx, Command::SetDpiLevels { levels: vec![900, 1200, 1600], active: 1 });
        cmd(&tx, Command::SetPolling(500));
        let all = drain(&urx, Duration::from_millis(150));
        assert!(!all.iter().any(|u| matches!(u, Update::Error(_))), "{all:?}");
        let calls = d.calls();
        let w = calls.iter().position(|c| c.starts_with("set_dpi_levels")).expect("edição perdida");
        assert!(w < calls.iter().position(|c| c.starts_with("set_polling_rate")).unwrap());
        finish(tx, h);
    }

    #[test]
    fn dropped_pending_edit_is_reported_when_the_daemon_is_truly_down() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_slow_debounce(d.clone());
        next_state(&urx);
        d.inner.lock().unwrap().running = false;
        tx.send(Msg::Event(Event::OwnerChanged(false))).unwrap();
        assert_eq!(next_state(&urx).status, DaemonStatus::Stopped);
        cmd(&tx, Command::SetDpiLevels { levels: vec![900, 1200, 1600], active: 1 });
        cmd(&tx, Command::SetPolling(500));
        let e1 = next_error(&urx);
        assert!(e1.contains("edição"), "{e1}");
        assert!(next_error(&urx).contains("não está rodando"));
        assert_eq!(count(&d, "set_dpi_levels"), 0);
        assert_eq!(count(&d, "set_polling_rate"), 0);
        finish(tx, h);
    }

    #[test]
    fn selection_is_kept_across_a_stopped_daemon() {
        let d = Arc::new(FakeDaemon::new());
        d.inner.lock().unwrap().devices.push(second_device());
        let (tx, urx, h) = rig(d.clone());
        next_state(&urx);
        cmd(&tx, Command::SelectDevice("dev-2".into()));
        assert_eq!(next_state(&urx).device.unwrap().id, "dev-2");
        d.inner.lock().unwrap().running = false;
        cmd(&tx, Command::Refresh);
        assert_eq!(next_state(&urx).status, DaemonStatus::Stopped);
        d.inner.lock().unwrap().running = true;
        cmd(&tx, Command::Refresh);
        assert_eq!(next_state(&urx).device.unwrap().id, "dev-2");
        finish(tx, h);
    }

    #[test]
    fn pending_edit_when_daemon_dies_is_reported_not_lost_silently() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_slow_debounce(d.clone());
        next_state(&urx);
        cmd(&tx, Command::SetDpiLevels { levels: vec![900, 1200, 1600], active: 1 });
        d.inner.lock().unwrap().running = false;
        let e = next_error(&urx); // a escrita vence e falha (sem pânico, sem travar)
        assert!(e.contains("não está rodando"), "{e}");
        assert_eq!(next_state(&urx).status, DaemonStatus::Stopped);
        finish(tx, h);
    }

    /// Daemon lento: cada escrita de polling demora; nada trava nem se perde a ordem.
    struct Slow(Arc<FakeDaemon>);
    impl DaemonApi for Slow {
        fn daemon_running(&self) -> bool { self.0.daemon_running() }
        fn start_daemon(&self) -> Result<(), String> { self.0.start_daemon() }
        fn devices(&self) -> Result<Vec<ghub_ipc::DeviceSummary>, ApiError> { self.0.devices() }
        fn get_device_state(&self, id: &str) -> Result<ghub_ipc::DeviceState, ApiError> { self.0.get_device_state(id) }
        fn set_dpi_levels(&self, id: &str, l: Vec<u32>, a: u32) -> Result<(), ApiError> { self.0.set_dpi_levels(id, l, a) }
        fn set_active_dpi_level(&self, id: &str, a: u32) -> Result<(), ApiError> { self.0.set_active_dpi_level(id, a) }
        fn set_polling_rate(&self, id: &str, hz: u32) -> Result<(), ApiError> {
            std::thread::sleep(Duration::from_millis(150));
            self.0.set_polling_rate(id, hz)
        }
        fn list_profiles(&self) -> Result<Vec<String>, ApiError> { self.0.list_profiles() }
        fn get_active_profile(&self) -> Result<String, ApiError> { self.0.get_active_profile() }
        fn activate_profile(&self, n: &str) -> Result<(), ApiError> { self.0.activate_profile(n) }
        fn create_profile(&self, n: &str) -> Result<(), ApiError> { self.0.create_profile(n) }
        fn delete_profile(&self, n: &str) -> Result<(), ApiError> { self.0.delete_profile(n) }
        fn rescan(&self) -> Result<(), ApiError> { self.0.rescan() }
        fn autostart(&self) -> Option<bool> { self.0.autostart() }
        fn set_autostart(&self, e: bool) -> Result<(), String> { self.0.set_autostart(e) }
    }

    #[test]
    fn slow_daemon_keeps_order_and_still_quits_cleanly() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, h) = rig_cfg(Arc::new(Slow(d.clone())),
            Config { debounce: Duration::from_millis(40), coalesce: Duration::from_millis(20) });
        next_state(&urx);
        cmd(&tx, Command::SetPolling(500));            // lento (150 ms)
        for i in 0..5u32 {                              // rajada durante a escrita lenta
            cmd(&tx, Command::SetDpiLevels { levels: vec![900 + i * 50, 1200, 1600], active: 1 });
        }
        cmd(&tx, Command::SetPolling(250));            // vai depois da edição pendente
        tx.send(Msg::Quit).unwrap();
        h.join().unwrap();
        let calls: Vec<String> = d.calls().into_iter().filter(|c| c.starts_with("set_")).collect();
        assert_eq!(calls, vec![
            "set_polling_rate dev-1 500".to_string(),
            "set_dpi_levels dev-1 [1100, 1200, 1600] 1".to_string(),
            "set_polling_rate dev-1 250".to_string(),
        ]);
    }
}
