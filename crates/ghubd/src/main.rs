use std::process::ExitCode;
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;

use ghub_ipc::{BUS_NAME, INTERFACE, OBJECT_PATH};
use ghubd::backend::SysBackend;
use ghubd::manager::{Command, Event, Manager};
use ghubd::profile::{store_path_from, ProfileStore};
use ghubd::service::DaemonService;
use ghubd::watcher;
use zbus::blocking::connection::Builder;
use zbus::blocking::Connection;

const TICK: Duration = Duration::from_secs(5);

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

    let conn = Builder::session()?
        .name(BUS_NAME)?
        .serve_at(OBJECT_PATH, DaemonService::new(cmd_tx.clone()))?
        .build()?;

    let signal_conn = conn.clone();
    thread::spawn(move || {
        for event in event_rx {
            emit(&signal_conn, event);
        }
    });

    watcher::spawn(cmd_tx.clone());

    let manager = Manager::new(SysBackend::new(), store, store_path, event_tx);
    eprintln!("ghubd: pronto em {BUS_NAME}");
    manager.run(cmd_rx, TICK);
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ghubd: {e}");
            ExitCode::FAILURE
        }
    }
}
