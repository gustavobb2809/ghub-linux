use std::io;
use std::os::fd::AsRawFd;
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::manager::Command;

const DEBOUNCE: Duration = Duration::from_millis(500);

/// Observa o subsistema `hidraw` e pede uma nova varredura quando algo é conectado ou removido.
///
/// O monitor é criado dentro da própria thread (ele não precisa ser `Send`).
pub fn spawn(tx: Sender<Command>) -> JoinHandle<()> {
    thread::spawn(move || {
        let monitor = match udev::MonitorBuilder::new()
            .and_then(|b| b.match_subsystem("hidraw"))
            .and_then(|b| b.listen())
        {
            Ok(m) => m,
            Err(e) => {
                eprintln!("ghubd: não foi possível iniciar o monitor udev: {e}");
                return;
            }
        };
        let fd = monitor.as_raw_fd();
        loop {
            let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            // SAFETY: `pfd` é válido durante a chamada e passamos 1 elemento.
            let rc = unsafe { libc::poll(&mut pfd, 1, -1) };
            if rc < 0 {
                if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                eprintln!("ghubd: erro no monitor udev: {}", io::Error::last_os_error());
                return;
            }
            let mut any = false;
            for _ in monitor.iter() {
                any = true;
            }
            if any {
                // Aguarda a rajada de eventos de uma conexão terminar e esvazia a fila.
                thread::sleep(DEBOUNCE);
                for _ in monitor.iter() {}
                if tx.send(Command::Rescan(None)).is_err() {
                    return;
                }
            }
        }
    })
}
