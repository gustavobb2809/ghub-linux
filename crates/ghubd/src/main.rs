use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ghub_ipc::{BUS_NAME, INTERFACE, OBJECT_PATH};
use ghubd::actions::{Action, Executor};
use ghubd::backend::SysBackend;
use ghubd::hub::{self, Wake};
use ghubd::link::{ChannelLink, GuiLauncher, ManagerLink, SystemctlControl};
use ghubd::manager::{Command, Event, Manager};
use ghubd::notify::{DbusNotifier, LogNotifier, Notification, Notifier};
use ghubd::profile::{store_path_from, ProfileStore};
use ghubd::service::DaemonService;
use ghubd::tray;
use ghubd::tray_model::{build_view, TrayInput};
use ghubd::watcher;
use zbus::blocking::connection::Builder;
use zbus::blocking::Connection;

const TICK: Duration = Duration::from_secs(5);

const USAGE: &str = "uso: ghubd [--notify-test | --help]\n\
  (sem argumentos)  executa o daemon\n\
  --notify-test     envia uma notificação de teste e sai\n";

fn emit(conn: &Connection, event: Event) {
    let result = match event {
        Event::DevicesChanged => {
            conn.emit_signal(None::<&str>, OBJECT_PATH, INTERFACE, "DevicesChanged", &())
        }
        Event::DeviceChanged(id) => {
            conn.emit_signal(None::<&str>, OBJECT_PATH, INTERFACE, "DeviceChanged", &id)
        }
        Event::ProfileChanged(name) => {
            conn.emit_signal(None::<&str>, OBJECT_PATH, INTERFACE, "ProfileChanged", &name)
        }
        Event::BatteryLow { id, percent } => conn.emit_signal(
            None::<&str>,
            OBJECT_PATH,
            INTERFACE,
            "BatteryLow",
            &(id, u32::from(percent)),
        ),
    };
    if let Err(e) = result {
        eprintln!("ghubd: falha ao emitir sinal: {e}");
    }
}

fn notify_test() -> Result<(), Box<dyn std::error::Error>> {
    let notifier = DbusNotifier::new()?;
    notifier.notify(&Notification::test());
    println!("notificação de teste enviada");
    Ok(())
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let xdg = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    let store_path = store_path_from(xdg.as_deref(), home.as_deref())
        .ok_or("não foi possível determinar o diretório de configuração (HOME)")?;
    let (store, warning) = ProfileStore::load_or_default(&store_path);
    if let Some(w) = warning {
        eprintln!("ghubd: aviso: {w}");
    }

    let (cmd_tx, cmd_rx) = channel::<Command>();
    let (event_tx, event_rx) = channel::<Event>();
    let (dbus_tx, dbus_rx) = channel::<Event>();
    let (wake_tx, wake_rx) = channel::<Wake>();
    let (action_tx, action_rx) = channel::<Action>();

    let conn = Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, DaemonService::new(cmd_tx.clone()))?
        .build()?;

    // Sinais D-Bus.
    let signal_conn = conn.clone();
    thread::spawn(move || {
        for event in dbus_rx {
            emit(&signal_conn, event);
        }
    });

    let link: Arc<dyn ManagerLink> = Arc::new(ChannelLink::new(cmd_tx.clone()));
    let service = Arc::new(SystemctlControl);
    let notifier: Arc<dyn Notifier> = match DbusNotifier::new() {
        Ok(n) => Arc::new(n),
        Err(e) => {
            eprintln!("ghubd: notificações indisponíveis ({e}); usando o log");
            Arc::new(LogNotifier)
        }
    };

    // Eventos do gerente -> D-Bus, bandeja e notificações.
    {
        let (link, notifier, wake_tx) = (link.clone(), notifier.clone(), wake_tx.clone());
        thread::spawn(move || {
            hub::run_fanout(event_rx, dbus_tx, wake_tx, &*link, &*notifier);
        });
    }

    // Bandeja (opcional): se falhar, o daemon segue sem ela.
    let initial = build_view(&TrayInput::empty());
    match tray::spawn(initial, action_tx) {
        Ok(handle) => {
            {
                let (link, service) = (link.clone(), service.clone());
                thread::spawn(move || {
                    hub::run_refresher(wake_rx, &*link, &*service, hub::PERIODIC, &mut |view| {
                        handle.update(|t| t.set_view(view));
                    });
                });
            }
            let exec = Executor::new(
                link.clone(),
                service.clone(),
                Arc::new(GuiLauncher),
                notifier.clone(),
            );
            let wake_tx = wake_tx.clone();
            thread::spawn(move || {
                hub::run_actions(action_rx, &exec, wake_tx);
                // "Sair": o gerente já foi avisado (ou o systemd vai parar o serviço).
                std::process::exit(0);
            });
        }
        Err(e) => eprintln!("ghubd: bandeja indisponível: {e}"),
    }

    watcher::spawn(cmd_tx.clone());

    let manager = Manager::new(SysBackend::new(), store, store_path, event_tx);
    eprintln!("ghubd: pronto em {BUS_NAME}");
    manager.run(cmd_rx, TICK);
    Ok(())
}

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        None => match run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ghubd: {e}");
                ExitCode::FAILURE
            }
        },
        Some("--notify-test") => match notify_test() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("ghubd: {e}");
                ExitCode::FAILURE
            }
        },
        Some("--help") | Some("-h") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("ghubd: argumento desconhecido: {other}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}
