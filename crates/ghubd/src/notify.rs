use std::collections::HashMap;

use zbus::blocking::Connection;
use zvariant::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Urgency {
    Low = 0,
    Normal = 1,
    Critical = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub summary: String,
    pub body: String,
    pub urgency: Urgency,
    pub icon: String,
}

impl Notification {
    pub fn battery_low(device_name: &str, percent: u8) -> Self {
        Notification {
            summary: "Bateria baixa".into(),
            body: format!("{device_name}: {percent} %"),
            urgency: Urgency::Critical,
            icon: "battery-caution".into(),
        }
    }

    pub fn error(text: &str) -> Self {
        Notification {
            summary: "G HUB Linux".into(),
            body: text.into(),
            urgency: Urgency::Normal,
            icon: "dialog-warning".into(),
        }
    }

    pub fn test() -> Self {
        Notification {
            summary: "G HUB Linux".into(),
            body: "Notificação de teste: se você está lendo isto, as notificações funcionam."
                .into(),
            urgency: Urgency::Low,
            icon: "dialog-information".into(),
        }
    }
}

pub trait Notifier: Send + Sync {
    fn notify(&self, n: &Notification);
}

/// Usado quando não há servidor de notificações: escreve no `stderr` (vai para o journal).
pub struct LogNotifier;

impl Notifier for LogNotifier {
    fn notify(&self, n: &Notification) {
        eprintln!("ghubd: notificação: {} — {}", n.summary, n.body);
    }
}

/// Envia `org.freedesktop.Notifications.Notify` pelo barramento de sessão.
pub struct DbusNotifier {
    conn: Connection,
}

impl DbusNotifier {
    pub fn new() -> Result<Self, zbus::Error> {
        Ok(DbusNotifier { conn: Connection::session()? })
    }
}

impl Notifier for DbusNotifier {
    fn notify(&self, n: &Notification) {
        let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
        hints.insert("urgency", Value::U8(n.urgency as u8));
        let result = self.conn.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "G HUB Linux",
                0u32,
                n.icon.as_str(),
                n.summary.as_str(),
                n.body.as_str(),
                Vec::<&str>::new(),
                hints,
                8000i32,
            ),
        );
        if let Err(e) = result {
            eprintln!("ghubd: falha ao enviar a notificação: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_low_message() {
        let n = Notification::battery_low("G703", 12);
        assert_eq!(n.summary, "Bateria baixa");
        assert_eq!(n.body, "G703: 12 %");
        assert_eq!(n.urgency, Urgency::Critical);
        assert_eq!(n.icon, "battery-caution");
    }

    #[test]
    fn error_and_test_messages() {
        let e = Notification::error("falhou");
        assert_eq!(e.summary, "G HUB Linux");
        assert_eq!(e.body, "falhou");
        assert_eq!(e.urgency, Urgency::Normal);
        assert_eq!(e.icon, "dialog-warning");
        let t = Notification::test();
        assert_eq!(t.urgency, Urgency::Low);
        assert!(t.body.contains("teste"));
    }

    #[test]
    fn urgency_values_match_the_freedesktop_spec() {
        assert_eq!(Urgency::Low as u8, 0);
        assert_eq!(Urgency::Normal as u8, 1);
        assert_eq!(Urgency::Critical as u8, 2);
    }

    #[test]
    fn log_notifier_does_not_panic() {
        LogNotifier.notify(&Notification::test());
    }
}
