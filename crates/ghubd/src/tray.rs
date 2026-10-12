use std::sync::mpsc::Sender;

use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Category, Icon, MenuItem, Status, ToolTip};

use crate::actions::Action;
use crate::tray_icon::{render, Pixmap, SIZES};
use crate::tray_model::{Entry, TrayView};

fn to_icon(p: Pixmap) -> Icon {
    Icon { width: p.size as i32, height: p.size as i32, data: p.argb }
}

pub struct GhubTray {
    view: TrayView,
    actions: Sender<Action>,
}

impl GhubTray {
    pub fn new(view: TrayView, actions: Sender<Action>) -> Self {
        GhubTray { view, actions }
    }

    pub fn set_view(&mut self, view: TrayView) {
        self.view = view;
    }
}

fn radio_options(labels: &[String]) -> Vec<RadioItem> {
    labels
        .iter()
        .map(|l| RadioItem { label: l.clone(), ..Default::default() })
        .collect()
}

fn menu_item(entry: &Entry, actions: &Sender<Action>) -> MenuItem<GhubTray> {
    match entry {
        Entry::Info(text) => StandardItem {
            label: text.clone(),
            enabled: false,
            ..Default::default()
        }
        .into(),
        Entry::Separator => MenuItem::Separator,
        Entry::Dpi { labels, selected, enabled } => {
            let tx = actions.clone();
            SubMenu {
                label: "DPI".into(),
                enabled: *enabled,
                submenu: vec![RadioGroup {
                    selected: *selected,
                    select: Box::new(move |_: &mut GhubTray, index| {
                        let _ = tx.send(Action::SetDpiLevel(index));
                    }),
                    options: radio_options(labels),
                    ..Default::default()
                }
                .into()],
                ..Default::default()
            }
            .into()
        }
        Entry::Profiles { labels, selected } => {
            let tx = actions.clone();
            let names = labels.clone();
            SubMenu {
                label: format!("Perfil: {}", labels.get(*selected).cloned().unwrap_or_default()),
                submenu: vec![RadioGroup {
                    selected: *selected,
                    select: Box::new(move |_: &mut GhubTray, index| {
                        if let Some(name) = names.get(index) {
                            let _ = tx.send(Action::ActivateProfile(name.clone()));
                        }
                    }),
                    options: radio_options(labels),
                    ..Default::default()
                }
                .into()],
                ..Default::default()
            }
            .into()
        }
        Entry::Autostart { checked, enabled } => {
            let tx = actions.clone();
            CheckmarkItem {
                label: "Iniciar com o sistema".into(),
                enabled: *enabled,
                checked: *checked,
                activate: Box::new(move |_: &mut GhubTray| {
                    let _ = tx.send(Action::ToggleAutostart);
                }),
                ..Default::default()
            }
            .into()
        }
        Entry::OpenGui => {
            let tx = actions.clone();
            StandardItem {
                label: "Abrir G HUB Linux".into(),
                activate: Box::new(move |_: &mut GhubTray| {
                    let _ = tx.send(Action::OpenGui);
                }),
                ..Default::default()
            }
            .into()
        }
        Entry::Quit => {
            let tx = actions.clone();
            StandardItem {
                label: "Sair e parar o serviço".into(),
                activate: Box::new(move |_: &mut GhubTray| {
                    let _ = tx.send(Action::Quit);
                }),
                ..Default::default()
            }
            .into()
        }
    }
}

impl ksni::Tray for GhubTray {
    fn id(&self) -> String {
        "ghub-linux".into()
    }

    fn title(&self) -> String {
        self.view.title.clone()
    }

    fn category(&self) -> Category {
        Category::Hardware
    }

    fn status(&self) -> Status {
        Status::Active
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        SIZES.iter().map(|s| to_icon(render(self.view.icon, *s))).collect()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: self.view.tooltip_title.clone(),
            description: self.view.tooltip_text.clone(),
            ..Default::default()
        }
    }

    /// Clique esquerdo: abre a interface gráfica.
    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.actions.send(Action::OpenGui);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.view
            .entries
            .iter()
            .map(|e| menu_item(e, &self.actions))
            .collect()
    }
}

pub type TrayHandle = ksni::blocking::Handle<GhubTray>;

/// Registra a bandeja. Com `assume_sni_available(true)` o `ksni` espera o
/// `StatusNotifierWatcher` aparecer (login antes do shell) em vez de falhar.
pub fn spawn(view: TrayView, actions: Sender<Action>) -> Result<TrayHandle, ksni::Error> {
    use ksni::blocking::TrayMethods;
    GhubTray::new(view, actions).assume_sni_available(true).spawn()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray_icon::IconState;
    use ksni::Tray;
    use std::sync::mpsc::{channel, Receiver};

    fn view(entries: Vec<Entry>) -> TrayView {
        TrayView {
            icon: IconState::Normal,
            title: "G HUB Linux".into(),
            tooltip_title: "G703".into(),
            tooltip_text: "91 %".into(),
            entries,
        }
    }

    fn full_view() -> TrayView {
        view(vec![
            Entry::Info("G703 — 91 %".into()),
            Entry::Separator,
            Entry::Dpi {
                labels: vec!["800 DPI".into(), "1200 DPI".into()],
                selected: 1,
                enabled: true,
            },
            Entry::Separator,
            Entry::Profiles {
                labels: vec!["Padrão".into(), "Trabalho".into()],
                selected: 0,
            },
            Entry::Separator,
            Entry::Autostart { checked: true, enabled: true },
            Entry::OpenGui,
            Entry::Quit,
        ])
    }

    fn tray(v: TrayView) -> (GhubTray, Receiver<Action>) {
        let (tx, rx) = channel();
        (GhubTray::new(v, tx), rx)
    }

    #[test]
    fn identity_and_tooltip() {
        let (t, _rx) = tray(full_view());
        assert_eq!(t.id(), "ghub-linux");
        assert_eq!(t.title(), "G HUB Linux");
        let tip = t.tool_tip();
        assert_eq!(tip.title, "G703");
        assert_eq!(tip.description, "91 %");
        assert_eq!(t.status(), Status::Active);
    }

    #[test]
    fn icon_pixmaps_cover_every_size() {
        let (t, _rx) = tray(full_view());
        let icons = t.icon_pixmap();
        assert_eq!(icons.len(), SIZES.len());
        for (icon, size) in icons.iter().zip(SIZES) {
            assert_eq!(icon.width, size as i32);
            assert_eq!(icon.height, size as i32);
            assert_eq!(icon.data.len(), (size * size * 4) as usize);
        }
    }

    #[test]
    fn menu_mirrors_the_view() {
        let (t, _rx) = tray(full_view());
        let menu = t.menu();
        assert_eq!(menu.len(), 9);
        match &menu[0] {
            MenuItem::Standard(i) => {
                assert_eq!(i.label, "G703 — 91 %");
                assert!(!i.enabled);
            }
            _ => panic!("esperava item informativo"),
        }
        assert!(matches!(menu[1], MenuItem::Separator));
        match &menu[2] {
            MenuItem::SubMenu(s) => {
                assert_eq!(s.label, "DPI");
                assert!(s.enabled);
                match &s.submenu[0] {
                    MenuItem::RadioGroup(g) => {
                        assert_eq!(g.selected, 1);
                        assert_eq!(g.options.len(), 2);
                        assert_eq!(g.options[1].label, "1200 DPI");
                    }
                    _ => panic!("esperava grupo de rádio"),
                }
            }
            _ => panic!("esperava submenu de DPI"),
        }
        match &menu[4] {
            MenuItem::SubMenu(s) => assert_eq!(s.label, "Perfil: Padrão"),
            _ => panic!("esperava submenu de perfil"),
        }
        match &menu[6] {
            MenuItem::Checkmark(c) => {
                assert_eq!(c.label, "Iniciar com o sistema");
                assert!(c.checked);
                assert!(c.enabled);
            }
            _ => panic!("esperava item marcável"),
        }
        match &menu[7] {
            MenuItem::Standard(i) => assert_eq!(i.label, "Abrir G HUB Linux"),
            _ => panic!(),
        }
        match &menu[8] {
            MenuItem::Standard(i) => assert_eq!(i.label, "Sair e parar o serviço"),
            _ => panic!(),
        }
    }

    #[test]
    fn disabled_dpi_and_autostart_are_reflected() {
        let (t, _rx) = tray(view(vec![
            Entry::Dpi { labels: vec!["800 DPI".into()], selected: 0, enabled: false },
            Entry::Autostart { checked: false, enabled: false },
        ]));
        let menu = t.menu();
        match &menu[0] {
            MenuItem::SubMenu(s) => assert!(!s.enabled),
            _ => panic!(),
        }
        match &menu[1] {
            MenuItem::Checkmark(c) => {
                assert!(!c.checked);
                assert!(!c.enabled);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn activations_send_the_right_actions() {
        let (mut t, rx) = tray(full_view());
        let menu = t.menu();

        if let MenuItem::SubMenu(s) = &menu[2] {
            if let MenuItem::RadioGroup(g) = &s.submenu[0] {
                (g.select)(&mut t, 0);
            }
        }
        assert_eq!(rx.try_recv().unwrap(), Action::SetDpiLevel(0));

        if let MenuItem::SubMenu(s) = &menu[4] {
            if let MenuItem::RadioGroup(g) = &s.submenu[0] {
                (g.select)(&mut t, 1);
            }
        }
        assert_eq!(rx.try_recv().unwrap(), Action::ActivateProfile("Trabalho".into()));

        if let MenuItem::Checkmark(c) = &menu[6] {
            (c.activate)(&mut t);
        }
        assert_eq!(rx.try_recv().unwrap(), Action::ToggleAutostart);

        if let MenuItem::Standard(i) = &menu[7] {
            (i.activate)(&mut t);
        }
        assert_eq!(rx.try_recv().unwrap(), Action::OpenGui);

        if let MenuItem::Standard(i) = &menu[8] {
            (i.activate)(&mut t);
        }
        assert_eq!(rx.try_recv().unwrap(), Action::Quit);

        t.activate(0, 0);
        assert_eq!(rx.try_recv().unwrap(), Action::OpenGui);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn set_view_replaces_what_is_shown() {
        let (mut t, _rx) = tray(full_view());
        let mut v = full_view();
        v.tooltip_text = "outro".into();
        t.set_view(v);
        assert_eq!(t.tool_tip().description, "outro");
    }
}
