use std::sync::Arc;

use crate::link::{Launcher, ManagerLink, ServiceControl};
use crate::notify::{Notification, Notifier};
use crate::tray_model::{pick_primary, Autostart};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Seleciona o nível de DPI de índice dado no perfil ativo.
    SetDpiLevel(usize),
    ActivateProfile(String),
    ToggleAutostart,
    OpenGui,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Reconstruir o menu a partir do estado atual.
    Refresh,
    /// Encerrar o processo.
    Exit,
}

pub struct Executor {
    manager: Arc<dyn ManagerLink>,
    service: Arc<dyn ServiceControl>,
    launcher: Arc<dyn Launcher>,
    notifier: Arc<dyn Notifier>,
}

impl Executor {
    pub fn new(
        manager: Arc<dyn ManagerLink>,
        service: Arc<dyn ServiceControl>,
        launcher: Arc<dyn Launcher>,
        notifier: Arc<dyn Notifier>,
    ) -> Self {
        Executor { manager, service, launcher, notifier }
    }

    fn fail(&self, text: &str) {
        self.notifier.notify(&Notification::error(text));
    }

    pub fn execute(&self, action: Action) -> Outcome {
        match action {
            Action::SetDpiLevel(level) => {
                let devices = self.manager.devices();
                match pick_primary(&devices) {
                    None => self.fail("Nenhum dispositivo conectado."),
                    Some(d) => {
                        if let Err(e) = self.manager.set_active_level(&d.id, level) {
                            self.fail(&format!("Não foi possível mudar o DPI: {e}"));
                        }
                    }
                }
                Outcome::Refresh
            }
            Action::ActivateProfile(name) => {
                if let Err(e) = self.manager.activate_profile(&name) {
                    self.fail(&format!("Não foi possível ativar o perfil {name}: {e}"));
                }
                Outcome::Refresh
            }
            Action::ToggleAutostart => {
                match self.service.autostart() {
                    Autostart::Unavailable => self.fail(
                        "O serviço do systemd não está instalado. Rode packaging/install-user.sh.",
                    ),
                    current => {
                        let enable = current == Autostart::Disabled;
                        if let Err(e) = self.service.set_autostart(enable) {
                            self.fail(&format!("Não foi possível alterar o início automático: {e}"));
                        }
                    }
                }
                Outcome::Refresh
            }
            Action::OpenGui => {
                if let Err(e) = self.launcher.open_gui() {
                    self.fail(&e);
                }
                Outcome::Refresh
            }
            Action::Quit => {
                match self.service.stop() {
                    Ok(true) => {}
                    Ok(false) => self.manager.shutdown(),
                    Err(e) => {
                        self.fail(&format!("Não foi possível parar o serviço: {e}"));
                        self.manager.shutdown();
                    }
                }
                Outcome::Exit
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::ManagerError;
    use crate::testing::{sample_summary, FakeLauncher, FakeLink, FakeNotifier, FakeService};

    struct Rig {
        link: Arc<FakeLink>,
        service: Arc<FakeService>,
        launcher: Arc<FakeLauncher>,
        notifier: Arc<FakeNotifier>,
        exec: Executor,
    }

    fn rig() -> Rig {
        let link = Arc::new(FakeLink::default());
        let service = Arc::new(FakeService::default());
        let launcher = Arc::new(FakeLauncher::default());
        let notifier = Arc::new(FakeNotifier::default());
        let exec = Executor::new(link.clone(), service.clone(), launcher.clone(), notifier.clone());
        Rig { link, service, launcher, notifier, exec }
    }

    fn with_device(r: &Rig) {
        *r.link.devices.lock().unwrap() =
            vec![sample_summary("off", false), sample_summary("on", true)];
    }

    #[test]
    fn set_dpi_level_targets_the_connected_device() {
        let r = rig();
        with_device(&r);
        assert_eq!(r.exec.execute(Action::SetDpiLevel(2)), Outcome::Refresh);
        assert_eq!(r.link.calls(), vec!["set_active_level:on:2".to_string()]);
        assert!(r.notifier.sent().is_empty());
    }

    #[test]
    fn set_dpi_level_without_a_device_notifies_and_writes_nothing() {
        let r = rig();
        assert_eq!(r.exec.execute(Action::SetDpiLevel(0)), Outcome::Refresh);
        assert!(r.link.calls().is_empty());
        assert_eq!(r.notifier.sent().len(), 1);
        assert!(r.notifier.sent()[0].body.contains("Nenhum dispositivo"));
    }

    #[test]
    fn manager_errors_become_notifications() {
        let r = rig();
        with_device(&r);
        *r.link.level_result.lock().unwrap() = Err(ManagerError::Offline("on".into()));
        r.exec.execute(Action::SetDpiLevel(1));
        *r.link.profile_result.lock().unwrap() = Err(ManagerError::NotFound("Zzz".into()));
        r.exec.execute(Action::ActivateProfile("Zzz".into()));
        let sent = r.notifier.sent();
        assert_eq!(sent.len(), 2);
        assert!(sent[0].body.contains("offline"));
        assert!(sent[1].body.contains("Zzz"));
    }

    #[test]
    fn activate_profile_calls_the_manager() {
        let r = rig();
        assert_eq!(
            r.exec.execute(Action::ActivateProfile("Trabalho".into())),
            Outcome::Refresh
        );
        assert_eq!(r.link.calls(), vec!["activate_profile:Trabalho".to_string()]);
        assert!(r.notifier.sent().is_empty());
    }

    #[test]
    fn toggle_autostart_flips_the_current_state() {
        let r = rig();
        *r.service.autostart.lock().unwrap() = Autostart::Disabled;
        r.exec.execute(Action::ToggleAutostart);
        *r.service.autostart.lock().unwrap() = Autostart::Enabled;
        r.exec.execute(Action::ToggleAutostart);
        assert_eq!(
            r.service.calls(),
            vec!["set_autostart:true".to_string(), "set_autostart:false".to_string()]
        );
        assert!(r.notifier.sent().is_empty());
    }

    #[test]
    fn toggle_autostart_when_unavailable_never_calls_systemctl() {
        let r = rig();
        *r.service.autostart.lock().unwrap() = Autostart::Unavailable;
        r.exec.execute(Action::ToggleAutostart);
        assert!(r.service.calls().is_empty());
        let sent = r.notifier.sent();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].body.contains("install-user.sh"));
    }

    #[test]
    fn toggle_autostart_failure_is_reported() {
        let r = rig();
        *r.service.set_result.lock().unwrap() = Err("Access denied".into());
        r.exec.execute(Action::ToggleAutostart);
        let sent = r.notifier.sent();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].body.contains("Access denied"));
    }

    #[test]
    fn open_gui_reports_failure_only() {
        let r = rig();
        assert_eq!(r.exec.execute(Action::OpenGui), Outcome::Refresh);
        assert_eq!(*r.launcher.opened.lock().unwrap(), 1);
        assert!(r.notifier.sent().is_empty());
        *r.launcher.result.lock().unwrap() = Err("não instalada".into());
        r.exec.execute(Action::OpenGui);
        assert!(r.notifier.sent()[0].body.contains("não instalada"));
    }

    #[test]
    fn quit_under_systemd_asks_systemd_and_exits() {
        let r = rig();
        *r.service.stop_result.lock().unwrap() = Ok(true);
        assert_eq!(r.exec.execute(Action::Quit), Outcome::Exit);
        assert_eq!(r.service.calls(), vec!["stop".to_string()]);
        assert!(!r.link.calls().contains(&"shutdown".to_string()));
    }

    #[test]
    fn quit_outside_systemd_shuts_the_manager_down() {
        let r = rig();
        *r.service.stop_result.lock().unwrap() = Ok(false);
        assert_eq!(r.exec.execute(Action::Quit), Outcome::Exit);
        assert!(r.link.calls().contains(&"shutdown".to_string()));
        assert!(r.notifier.sent().is_empty());
    }

    #[test]
    fn quit_still_exits_when_stopping_the_service_fails() {
        let r = rig();
        *r.service.stop_result.lock().unwrap() = Err("falhou".into());
        assert_eq!(r.exec.execute(Action::Quit), Outcome::Exit);
        assert!(r.link.calls().contains(&"shutdown".to_string()));
        assert!(r.notifier.sent()[0].body.contains("falhou"));
    }
}
