use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use crate::actions::{Action, Executor, Outcome};
use crate::link::{ManagerLink, ServiceControl};
use crate::manager::Event;
use crate::notify::{Notification, Notifier};
use crate::tray_model::{build_view, pick_primary, TrayInput, TrayView};

/// Pedido de atualização da bandeja (o conteúdo é sempre relido do gerente).
#[derive(Debug, Clone, Copy)]
pub struct Wake;

/// Espera por mais eventos antes de reconstruir o menu, para juntar rajadas.
pub const COALESCE: Duration = Duration::from_millis(150);
/// Intervalo da atualização periódica (também relê o estado do systemd).
pub const PERIODIC: Duration = Duration::from_secs(30);

pub fn collect_input(link: &dyn ManagerLink, service: &dyn ServiceControl) -> TrayInput {
    let devices = link.devices();
    let primary = pick_primary(&devices).and_then(|d| link.state(&d.id));
    TrayInput {
        devices,
        primary,
        profiles: link.profiles(),
        active_profile: link.active_profile(),
        autostart: service.autostart(),
    }
}

/// Distribui os eventos do gerente: D-Bus (sinais), bandeja (atualização) e notificações.
/// Retorna quando o canal de eventos fecha.
pub fn run_fanout(
    events: Receiver<Event>,
    to_dbus: Sender<Event>,
    to_tray: Sender<Wake>,
    link: &dyn ManagerLink,
    notifier: &dyn Notifier,
) {
    for event in events {
        if let Event::BatteryLow { id, percent } = &event {
            let name = link
                .devices()
                .into_iter()
                .find(|d| d.id == *id)
                .map(|d| d.name)
                .unwrap_or_else(|| id.clone());
            notifier.notify(&Notification::battery_low(&name, *percent));
        }
        let _ = to_dbus.send(event);
        let _ = to_tray.send(Wake);
    }
}

/// Mantém a bandeja em dia: aplica a vista inicial e reconstrói o menu quando há um pedido
/// (juntando rajadas) ou a cada `periodic`. Retorna quando o canal de pedidos fecha.
pub fn run_refresher(
    wake: Receiver<Wake>,
    link: &dyn ManagerLink,
    service: &dyn ServiceControl,
    periodic: Duration,
    apply: &mut dyn FnMut(TrayView),
) {
    apply(build_view(&collect_input(link, service)));
    loop {
        match wake.recv_timeout(periodic) {
            Ok(Wake) => while wake.recv_timeout(COALESCE).is_ok() {},
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        apply(build_view(&collect_input(link, service)));
    }
}

/// Executa as ações da bandeja. Retorna quando uma ação pede para encerrar.
pub fn run_actions(actions: Receiver<Action>, exec: &Executor, wake: Sender<Wake>) {
    for action in actions {
        match exec.execute(action) {
            Outcome::Refresh => {
                let _ = wake.send(Wake);
            }
            Outcome::Exit => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{
        sample_state, sample_summary, FakeLauncher, FakeLink, FakeNotifier, FakeService,
    };
    use crate::tray_icon::IconState;
    use crate::tray_model::{Autostart, Entry};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};
    use std::thread;

    #[test]
    fn collect_input_gathers_everything_for_the_primary_device() {
        let link = FakeLink::default();
        *link.devices.lock().unwrap() = vec![sample_summary("a", false), sample_summary("b", true)];
        *link.state.lock().unwrap() = Some(sample_state("b", true));
        *link.profiles.lock().unwrap() = vec!["Padrão".into(), "Trabalho".into()];
        *link.active.lock().unwrap() = "Trabalho".into();
        let service = FakeService::default();
        *service.autostart.lock().unwrap() = Autostart::Enabled;
        let input = collect_input(&link, &service);
        assert_eq!(input.devices.len(), 2);
        assert_eq!(input.primary.unwrap().id, "b");
        assert_eq!(input.profiles.len(), 2);
        assert_eq!(input.active_profile, "Trabalho");
        assert_eq!(input.autostart, Autostart::Enabled);
    }

    #[test]
    fn collect_input_with_nothing_connected_is_empty_but_valid() {
        let input = collect_input(&FakeLink::default(), &FakeService::default());
        assert!(input.devices.is_empty());
        assert!(input.primary.is_none());
        let view = build_view(&input);
        assert_eq!(view.icon, IconState::Disconnected);
    }

    #[test]
    fn fanout_forwards_every_event_and_notifies_battery_low_once() {
        let link = FakeLink::default();
        *link.devices.lock().unwrap() = vec![sample_summary("dev-1", true)];
        let notifier = FakeNotifier::default();
        let (ev_tx, ev_rx) = channel();
        let (dbus_tx, dbus_rx) = channel();
        let (wake_tx, wake_rx) = channel();
        ev_tx.send(Event::DevicesChanged).unwrap();
        ev_tx.send(Event::BatteryLow { id: "dev-1".into(), percent: 12 }).unwrap();
        ev_tx.send(Event::DeviceChanged("dev-1".into())).unwrap();
        drop(ev_tx);
        run_fanout(ev_rx, dbus_tx, wake_tx, &link, &notifier);

        let forwarded: Vec<Event> = dbus_rx.try_iter().collect();
        assert_eq!(forwarded.len(), 3);
        assert_eq!(forwarded[0], Event::DevicesChanged);
        assert_eq!(wake_rx.try_iter().count(), 3);
        let sent = notifier.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].body, "Mouse dev-1: 12 %");
    }

    #[test]
    fn fanout_falls_back_to_the_id_when_the_device_is_unknown() {
        let link = FakeLink::default();
        let notifier = FakeNotifier::default();
        let (ev_tx, ev_rx) = channel();
        let (dbus_tx, _dbus_rx) = channel();
        let (wake_tx, _wake_rx) = channel();
        ev_tx.send(Event::BatteryLow { id: "x".into(), percent: 9 }).unwrap();
        drop(ev_tx);
        run_fanout(ev_rx, dbus_tx, wake_tx, &link, &notifier);
        assert_eq!(notifier.sent()[0].body, "x: 9 %");
    }

    #[test]
    fn refresher_applies_the_initial_view_and_coalesces_bursts() {
        let link = FakeLink::default();
        let service = FakeService::default();
        let (wake_tx, wake_rx) = channel();
        for _ in 0..5 {
            wake_tx.send(Wake).unwrap();
        }
        drop(wake_tx);
        let mut applied = 0;
        run_refresher(wake_rx, &link, &service, Duration::from_secs(60), &mut |_| applied += 1);
        // vista inicial + uma única atualização para a rajada
        assert_eq!(applied, 2);
    }

    #[test]
    fn refresher_also_refreshes_periodically() {
        let link = Arc::new(FakeLink::default());
        let service = Arc::new(FakeService::default());
        let (wake_tx, wake_rx) = channel::<Wake>();
        let (view_tx, view_rx) = channel::<TrayView>();
        let l = link.clone();
        let s = service.clone();
        let handle = thread::spawn(move || {
            run_refresher(wake_rx, &*l, &*s, Duration::from_millis(30), &mut |v| {
                let _ = view_tx.send(v);
            });
        });
        for _ in 0..3 {
            view_rx.recv_timeout(Duration::from_secs(2)).expect("atualização periódica");
        }
        drop(wake_tx);
        handle.join().unwrap();
    }

    #[test]
    fn refresher_reflects_new_state_on_the_next_refresh() {
        let link = Arc::new(FakeLink::default());
        let service = Arc::new(FakeService::default());
        let seen: Arc<Mutex<Vec<IconState>>> = Arc::new(Mutex::new(Vec::new()));
        let (wake_tx, wake_rx) = channel::<Wake>();
        let (l, s, sn) = (link.clone(), service.clone(), seen.clone());
        let handle = thread::spawn(move || {
            run_refresher(wake_rx, &*l, &*s, Duration::from_secs(60), &mut |v| {
                sn.lock().unwrap().push(v.icon);
            });
        });
        thread::sleep(Duration::from_millis(100));
        *link.devices.lock().unwrap() = vec![sample_summary("a", true)];
        *link.state.lock().unwrap() = Some(sample_state("a", true));
        wake_tx.send(Wake).unwrap();
        thread::sleep(Duration::from_millis(500));
        drop(wake_tx);
        handle.join().unwrap();
        let seen = seen.lock().unwrap().clone();
        assert_eq!(seen.first(), Some(&IconState::Disconnected));
        assert_eq!(seen.last(), Some(&IconState::Normal));
    }

    #[test]
    fn actions_loop_wakes_the_refresher_and_stops_on_quit() {
        let link = Arc::new(FakeLink::default());
        *link.devices.lock().unwrap() = vec![sample_summary("on", true)];
        let service = Arc::new(FakeService::default());
        // Fora do systemd (`stop` devolve Ok(false)): o "Sair" chama `shutdown` no gerente.
        *service.stop_result.lock().unwrap() = Ok(false);
        let launcher = Arc::new(FakeLauncher::default());
        let notifier = Arc::new(FakeNotifier::default());
        let exec = Executor::new(link.clone(), service, launcher, notifier);
        let (act_tx, act_rx) = channel();
        let (wake_tx, wake_rx) = channel();
        act_tx.send(Action::SetDpiLevel(0)).unwrap();
        act_tx.send(Action::OpenGui).unwrap();
        act_tx.send(Action::Quit).unwrap();
        act_tx.send(Action::OpenGui).unwrap(); // depois do Quit: não deve ser executado
        run_actions(act_rx, &exec, wake_tx);
        assert_eq!(wake_rx.try_iter().count(), 2);
        assert_eq!(link.calls(), vec!["set_active_level:on:0".to_string(), "shutdown".to_string()]);
    }

    #[test]
    fn unused_entry_import_is_exercised() {
        // Garante que o modelo continua expondo as entradas que o hub repassa à bandeja.
        let view = build_view(&TrayInput::empty());
        assert!(view.entries.contains(&Entry::Quit));
    }
}
