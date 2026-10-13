//! Instancia unica da GUI: o primeiro processo serve `org.ghublinux.Gui1`;
//! os seguintes chamam `Activate` no dono e saem.

use std::sync::mpsc::{channel, Receiver, Sender};

pub const GUI_BUS_NAME: &str = "org.ghublinux.Gui";
pub const GUI_PATH: &str = "/org/ghublinux/Gui";
const GUI_INTERFACE: &str = "org.ghublinux.Gui1";

struct GuiService {
    tx: Sender<()>,
}

#[zbus::interface(name = "org.ghublinux.Gui1")]
impl GuiService {
    fn activate(&self) {
        // Receptor descartado (GUI encerrando): ignora sem panico.
        let _ = self.tx.send(());
    }
}

pub struct Primary {
    _conn: zbus::blocking::Connection,
    pub activations: Receiver<()>,
}
pub enum Acquired {
    Primary(Primary),
    Other,
}

/// Tenta ser a instancia unica. Se o nome ja tem dono, pede a ele que mostre a janela.
pub fn acquire(name: &str, path: &str) -> Result<Acquired, zbus::Error> {
    let (tx, rx) = channel();
    // O Builder do zbus NAO ativa DoNotQueue (apesar da doc): um segundo
    // requisitante entraria na fila e `build()` retornaria Ok. Por isso o
    // nome e pedido explicitamente com DoNotQueue, depois de registrar o
    // objeto (serve_at), e `Exists` vira `NameTaken`.
    let built = zbus::blocking::connection::Builder::session()?
        .serve_at(path.to_string(), GuiService { tx })?
        .build()
        .and_then(|conn| {
            conn.request_name_with_flags(name.to_string(), zbus::fdo::RequestNameFlags::DoNotQueue.into())
                .map(|_| conn)
        });
    match built {
        Ok(conn) => Ok(Acquired::Primary(Primary { _conn: conn, activations: rx })),
        Err(zbus::Error::NameTaken) => {
            let conn = zbus::blocking::connection::Builder::session()?
                .method_timeout(super::real::CALL_TIMEOUT)
                .build()?;
            conn.call_method(Some(name.to_string()), path.to_string(), Some(GUI_INTERFACE), "Activate", &())?;
            Ok(Acquired::Other)
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    #[ignore = "needs a session bus"]
    fn second_acquire_activates_the_first() {
        let name = format!("org.ghublinux.GuiTest{}", std::process::id());
        let first = acquire(&name, "/t").expect("first");
        let Acquired::Primary(p) = first else { panic!("first should be primary") };
        // Segunda tentativa na mesma thread/processo: usa outra conexao, entao
        // o nome colide como em um segundo processo.
        let second = acquire(&name, "/t").expect("second");
        assert!(matches!(second, Acquired::Other));
        assert!(p.activations.recv_timeout(Duration::from_secs(2)).is_ok());
    }
}
