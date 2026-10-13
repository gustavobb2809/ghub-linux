use std::sync::mpsc::{self, Sender};
use std::time::Duration;

use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Category, Icon, MenuItem, Status, ToolTip};

use crate::actions::{Action, Request};
use crate::tray_icon::{render, Pixmap, SIZES};
use crate::tray_model::{Entry, TrayView};

fn to_icon(p: Pixmap) -> Icon {
    Icon { width: p.size as i32, height: p.size as i32, data: p.argb }
}

/// Quanto um clique que muda o estado espera pela vista real antes de devolver o menu.
pub const SETTLE_TIMEOUT: Duration = Duration::from_millis(800);

pub struct GhubTray {
    view: TrayView,
    actions: Sender<Request>,
    settle_timeout: Duration,
    /// Conta as mudanças publicadas da vista; a paridade decide os filhos do marcador de
    /// layout (ver `layout_marker`).
    epoch: u32,
}

impl GhubTray {
    pub fn new(view: TrayView, actions: Sender<Request>) -> Self {
        GhubTray { view, actions, settle_timeout: SETTLE_TIMEOUT, epoch: 0 }
    }

    pub fn with_settle_timeout(mut self, timeout: Duration) -> Self {
        self.settle_timeout = timeout;
        self
    }

    /// Substituição autoritativa: o refresh real sobrescreve qualquer estado otimista.
    pub fn set_view(&mut self, view: TrayView) {
        if view != self.view {
            self.epoch = self.epoch.wrapping_add(1);
        }
        self.view = view;
    }

    /// Fecha um clique: uma única virada de época se a vista final (otimista e/ou
    /// respondida) difere da de antes do clique.
    fn finish_click(&mut self, before: &TrayView) {
        if self.view != *before {
            self.epoch = self.epoch.wrapping_add(1);
        }
    }

    // Atualizações otimistas: o `ksni` regera o menu logo após rodar o closure, então o
    // host já recebe o novo estado na mesma ida ao D-Bus (sem esperar o refresh assíncrono).

    fn select_dpi(&mut self, index: usize) {
        if let Some(Entry::Dpi { labels, selected, .. }) =
            self.view.entries.iter_mut().find(|e| matches!(e, Entry::Dpi { .. }))
        {
            if index < labels.len() {
                *selected = index;
            }
        }
    }

    fn select_profile(&mut self, index: usize) {
        if let Some(Entry::Profiles { labels, selected }) =
            self.view.entries.iter_mut().find(|e| matches!(e, Entry::Profiles { .. }))
        {
            if index < labels.len() {
                *selected = index;
            }
        }
    }

    fn toggle_autostart(&mut self) {
        if let Some(Entry::Autostart { checked, enabled }) =
            self.view.entries.iter_mut().find(|e| matches!(e, Entry::Autostart { .. }))
        {
            if *enabled {
                *checked = !*checked;
            }
        }
    }
}

/// O dbusmenu usa `_` para marcar a tecla de acesso: um `_` sozinho some e `__` vira `_`.
/// Escapa o texto vindo do usuário (nomes de perfil, rótulos de DPI, nome do dispositivo).
fn escape(text: &str) -> String {
    text.replace('_', "__")
}

/// Envia uma ação que muda o estado (o estado otimista já foi aplicado) e espera, até
/// `timeout`, a vista reconstruída depois de executá-la. O host (ex.: Quickshell) relê o
/// menu com `GetLayout` poucos ms após o clique e ignora o `ItemsPropertiesUpdated` posterior,
/// então a resposta tem de sair já com o estado real (bateria, DPI, Hz…). Sem resposta, fica
/// o estado otimista. Roda com a trava do `ksni` tomada: não usar `handle.update` aqui.
fn settle(this: &mut GhubTray, tx: &Sender<Request>, timeout: Duration, action: Action) {
    let (reply, view) = mpsc::channel::<TrayView>();
    if tx.send(Request { action, reply: Some(reply) }).is_err() {
        return;
    }
    if let Ok(v) = view.recv_timeout(timeout) {
        this.view = v;
    }
}

/// Marcador de layout: submenu oculto no fim da raiz cuja lista de filhos alterna entre
/// vazia e um filho oculto a cada época. O `ksni` só emite `LayoutUpdated` quando a lista de
/// filhos de algum item muda, e alguns hosts (ex.: Quickshell) ignoram `toggle-state` vindo
/// por `ItemsPropertiesUpdated`; forçar o relayout faz o host reler o menu inteiro.
/// Risco: um host que ignore `visible:false` mostraria uma linha vazia no fim (verificado no Quickshell do usuário: não mostra).
fn layout_marker(epoch: u32) -> MenuItem<GhubTray> {
    let submenu = if epoch % 2 == 1 {
        vec![StandardItem {
            label: String::new(),
            visible: false,
            enabled: false,
            ..Default::default()
        }
        .into()]
    } else {
        vec![]
    };
    SubMenu { label: String::new(), visible: false, enabled: false, submenu, ..Default::default() }
        .into()
}

/// Rótulo do submenu de DPI com o valor atual ("DPI: 1200"), como o "DPI … ativo" do mock.
fn dpi_label(labels: &[String], selected: usize) -> String {
    match labels.get(selected) {
        Some(l) => format!("DPI: {}", escape(l.strip_suffix(" DPI").unwrap_or(l))),
        None => "DPI".into(),
    }
}

fn radio_options(labels: &[String]) -> Vec<RadioItem> {
    labels
        .iter()
        .map(|l| RadioItem { label: escape(l), ..Default::default() })
        .collect()
}

fn menu_item(entry: &Entry, actions: &Sender<Request>, timeout: Duration) -> MenuItem<GhubTray> {
    match entry {
        Entry::Info(text) => StandardItem {
            label: escape(text),
            enabled: false,
            ..Default::default()
        }
        .into(),
        Entry::Separator => MenuItem::Separator,
        Entry::Dpi { labels, selected, enabled } => {
            let tx = actions.clone();
            SubMenu {
                label: dpi_label(labels, *selected),
                enabled: *enabled,
                submenu: vec![RadioGroup {
                    selected: *selected,
                    select: Box::new(move |this: &mut GhubTray, index| {
                        let before = this.view.clone();
                        this.select_dpi(index);
                        settle(this, &tx, timeout, Action::SetDpiLevel(index));
                        this.finish_click(&before);
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
                label: format!("Perfil: {}", escape(labels.get(*selected).map_or("", String::as_str))),
                submenu: vec![RadioGroup {
                    selected: *selected,
                    select: Box::new(move |this: &mut GhubTray, index| {
                        if let Some(name) = names.get(index) {
                            let before = this.view.clone();
                            this.select_profile(index);
                            settle(this, &tx, timeout, Action::ActivateProfile(name.clone()));
                            this.finish_click(&before);
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
                activate: Box::new(move |this: &mut GhubTray| {
                    let before = this.view.clone();
                    this.toggle_autostart();
                    settle(this, &tx, timeout, Action::ToggleAutostart);
                    this.finish_click(&before);
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
                    let _ = tx.send(Request { action: Action::OpenGui, reply: None });
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
                    let _ = tx.send(Request { action: Action::Quit, reply: None });
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
        let _ = self.actions.send(Request { action: Action::OpenGui, reply: None });
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.view
            .entries
            .iter()
            .map(|e| menu_item(e, &self.actions, self.settle_timeout))
            .chain(std::iter::once(layout_marker(self.epoch)))
            .collect()
    }
}

pub type TrayHandle = ksni::blocking::Handle<GhubTray>;

/// Registra a bandeja. Com `assume_sni_available(true)` o `ksni` espera o
/// `StatusNotifierWatcher` aparecer (login antes do shell) em vez de falhar.
pub fn spawn(view: TrayView, actions: Sender<Request>) -> Result<TrayHandle, ksni::Error> {
    use ksni::blocking::TrayMethods;
    GhubTray::new(view, actions).assume_sni_available(true).spawn()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tray_icon::IconState;
    use ksni::Tray;
    use std::sync::mpsc::{channel, Receiver};
    use std::thread;
    use std::time::Instant;

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
            Entry::Info("G703".into()),
            Entry::Info("91 % · conectado · perfil Padrão".into()),
            Entry::Separator,
            Entry::Dpi {
                labels: vec!["800 DPI".into(), "1200 DPI".into()],
                selected: 1,
                enabled: true,
            },
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

    /// Bandeja de teste: ninguém responde aos pedidos (o receptor fica vivo e calado), então o
    /// prazo de espera é curto para os testes não demorarem.
    fn tray(v: TrayView) -> (GhubTray, Receiver<Request>) {
        let (tx, rx) = channel();
        (GhubTray::new(v, tx).with_settle_timeout(Duration::from_millis(30)), rx)
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
        // 9 entradas da vista + o marcador de layout oculto no fim.
        assert_eq!(menu.len(), 9 + 1);
        assert_eq!(marker_children(&t), 0);
        match &menu[0] {
            MenuItem::Standard(i) => {
                assert_eq!(i.label, "G703");
                assert!(!i.enabled);
            }
            _ => panic!("esperava item informativo"),
        }
        match &menu[1] {
            MenuItem::Standard(i) => {
                assert_eq!(i.label, "91 % · conectado · perfil Padrão");
                assert!(!i.enabled);
            }
            _ => panic!("esperava item informativo"),
        }
        assert!(matches!(menu[5], MenuItem::Separator));
        assert!(matches!(menu[2], MenuItem::Separator));
        match &menu[3] {
            MenuItem::SubMenu(s) => {
                assert_eq!(s.label, "DPI: 1200");
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

        if let MenuItem::SubMenu(s) = &menu[3] {
            if let MenuItem::RadioGroup(g) = &s.submenu[0] {
                (g.select)(&mut t, 0);
            }
        }
        assert_eq!(rx.try_recv().unwrap().action, Action::SetDpiLevel(0));

        if let MenuItem::SubMenu(s) = &menu[4] {
            if let MenuItem::RadioGroup(g) = &s.submenu[0] {
                (g.select)(&mut t, 1);
            }
        }
        assert_eq!(rx.try_recv().unwrap().action, Action::ActivateProfile("Trabalho".into()));

        if let MenuItem::Checkmark(c) = &menu[6] {
            (c.activate)(&mut t);
        }
        assert_eq!(rx.try_recv().unwrap().action, Action::ToggleAutostart);

        if let MenuItem::Standard(i) = &menu[7] {
            (i.activate)(&mut t);
        }
        assert_eq!(rx.try_recv().unwrap().action, Action::OpenGui);

        if let MenuItem::Standard(i) = &menu[8] {
            (i.activate)(&mut t);
        }
        assert_eq!(rx.try_recv().unwrap().action, Action::Quit);

        t.activate(0, 0);
        assert_eq!(rx.try_recv().unwrap().action, Action::OpenGui);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn underscores_in_user_text_are_escaped_but_actions_keep_the_original() {
        let (mut t, rx) = tray(view(vec![
            Entry::Info("G_703 — 91 %".into()),
            Entry::Dpi { labels: vec!["800_DPI".into()], selected: 0, enabled: true },
            Entry::Profiles { labels: vec!["a_b".into(), "c".into()], selected: 0 },
            Entry::OpenGui,
        ]));
        let menu = t.menu();
        match &menu[0] {
            MenuItem::Standard(i) => assert_eq!(i.label, "G__703 — 91 %"),
            _ => panic!("esperava item informativo"),
        }
        match &menu[1] {
            MenuItem::SubMenu(s) => {
                assert_eq!(s.label, "DPI: 800__DPI");
                match &s.submenu[0] {
                    MenuItem::RadioGroup(g) => assert_eq!(g.options[0].label, "800__DPI"),
                    _ => panic!("esperava grupo de rádio"),
                }
            }
            _ => panic!("esperava submenu de DPI"),
        }
        match &menu[2] {
            MenuItem::SubMenu(s) => {
                assert_eq!(s.label, "Perfil: a__b");
                match &s.submenu[0] {
                    MenuItem::RadioGroup(g) => {
                        assert_eq!(g.options[0].label, "a__b");
                        assert_eq!(g.options[1].label, "c");
                        (g.select)(&mut t, 0);
                    }
                    _ => panic!("esperava grupo de rádio"),
                }
            }
            _ => panic!("esperava submenu de perfil"),
        }
        assert_eq!(rx.try_recv().unwrap().action, Action::ActivateProfile("a_b".into()));
        match &menu[3] {
            MenuItem::Standard(i) => assert_eq!(i.label, "Abrir G HUB Linux"),
            _ => panic!(),
        }
    }

    #[test]
    fn escape_doubles_every_underscore() {
        assert_eq!(escape("a_b"), "a__b");
        assert_eq!(escape("__x_"), "____x__");
        assert_eq!(escape("sem sublinhado"), "sem sublinhado");
    }

    #[test]
    fn set_view_replaces_what_is_shown() {
        let (mut t, _rx) = tray(full_view());
        let mut v = full_view();
        v.tooltip_text = "outro".into();
        t.set_view(v);
        assert_eq!(t.tool_tip().description, "outro");
    }

    fn radio_selected(item: &MenuItem<GhubTray>) -> usize {
        match item {
            MenuItem::SubMenu(s) => match &s.submenu[0] {
                MenuItem::RadioGroup(g) => g.selected,
                _ => panic!("esperava grupo de rádio"),
            },
            _ => panic!("esperava submenu"),
        }
    }

    fn select_radio(t: &mut GhubTray, pos: usize, index: usize) {
        let menu = t.menu();
        match &menu[pos] {
            MenuItem::SubMenu(s) => match &s.submenu[0] {
                MenuItem::RadioGroup(g) => (g.select)(t, index),
                _ => panic!("esperava grupo de rádio"),
            },
            _ => panic!("esperava submenu"),
        }
    }

    fn activate_checkmark(t: &mut GhubTray, pos: usize) {
        let menu = t.menu();
        match &menu[pos] {
            MenuItem::Checkmark(c) => (c.activate)(t),
            _ => panic!("esperava item marcável"),
        }
    }

    #[test]
    fn dpi_click_updates_the_menu_optimistically() {
        let (mut t, rx) = tray(view(vec![Entry::Dpi {
            labels: vec!["400 DPI".into(), "800 DPI".into(), "1600 DPI".into()],
            selected: 0,
            enabled: true,
        }]));
        select_radio(&mut t, 0, 2);
        assert_eq!(radio_selected(&t.menu()[0]), 2);
        assert_eq!(rx.try_recv().unwrap().action, Action::SetDpiLevel(2));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn profile_click_updates_the_menu_optimistically() {
        let (mut t, rx) = tray(view(vec![Entry::Profiles {
            labels: vec!["Padrão".into(), "a_b".into()],
            selected: 0,
        }]));
        select_radio(&mut t, 0, 1);
        let menu = t.menu();
        match &menu[0] {
            MenuItem::SubMenu(s) => assert_eq!(s.label, "Perfil: a__b"),
            _ => panic!("esperava submenu de perfil"),
        }
        assert_eq!(radio_selected(&menu[0]), 1);
        assert_eq!(rx.try_recv().unwrap().action, Action::ActivateProfile("a_b".into()));
        assert!(rx.try_recv().is_err());

        let before = t.view.clone();
        select_radio(&mut t, 0, 5);
        assert_eq!(t.view, before);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn autostart_click_flips_the_checkmark_optimistically() {
        let (mut t, rx) = tray(full_view());
        activate_checkmark(&mut t, 6);
        match &t.menu()[6] {
            MenuItem::Checkmark(c) => assert!(!c.checked),
            _ => panic!("esperava item marcável"),
        }
        assert_eq!(rx.try_recv().unwrap().action, Action::ToggleAutostart);

        let (mut t, _rx) = tray(view(vec![Entry::Autostart { checked: true, enabled: false }]));
        let before = t.view.clone();
        activate_checkmark(&mut t, 0);
        assert_eq!(t.view, before);
        match &t.menu()[0] {
            MenuItem::Checkmark(c) => assert!(c.checked),
            _ => panic!("esperava item marcável"),
        }
    }

    #[test]
    fn real_refresh_overrides_the_optimistic_state() {
        let original = full_view();
        let (mut t, _rx) = tray(original.clone());
        select_radio(&mut t, 3, 0);
        select_radio(&mut t, 4, 1);
        activate_checkmark(&mut t, 6);
        assert_ne!(t.view, original);
        t.set_view(original.clone());
        let menu = t.menu();
        assert_eq!(radio_selected(&menu[3]), 1);
        assert_eq!(radio_selected(&menu[4]), 0);
        match &menu[6] {
            MenuItem::Checkmark(c) => assert!(c.checked),
            _ => panic!("esperava item marcável"),
        }
    }

    /// Faz o papel do laço de ações: para cada pedido com resposta, devolve `reply_view`
    /// (uma vista reconhecível, diferente da otimista). Devolve os pedidos recebidos.
    fn answering_actions(
        rx: Receiver<Request>,
        reply_view: TrayView,
    ) -> thread::JoinHandle<Vec<(Action, bool)>> {
        thread::spawn(move || {
            let mut seen = Vec::new();
            for req in rx {
                let has_reply = req.reply.is_some();
                if let Some(reply) = req.reply {
                    let _ = reply.send(reply_view.clone());
                }
                seen.push((req.action, has_reply));
            }
            seen
        })
    }

    fn settled_view() -> TrayView {
        let mut v = view(vec![
            Entry::Info("G703".into()),
            Entry::Info("42 % · conectado · perfil Trabalho".into()),
            Entry::Separator,
            Entry::Dpi {
                labels: vec!["800 DPI".into(), "1200 DPI".into()],
                selected: 0,
                enabled: true,
            },
            Entry::Profiles {
                labels: vec!["Padrão".into(), "Trabalho".into()],
                selected: 1,
            },
            Entry::Separator,
            Entry::Autostart { checked: false, enabled: true },
            Entry::OpenGui,
            Entry::Quit,
        ]);
        v.tooltip_text = "vista real".into();
        v
    }

    fn info_label(item: &MenuItem<GhubTray>) -> String {
        match item {
            MenuItem::Standard(i) => i.label.clone(),
            _ => panic!("esperava item informativo"),
        }
    }

    fn assert_shows_settled_view(t: &GhubTray) {
        assert_eq!(t.tool_tip().description, "vista real");
        let menu = t.menu();
        assert_eq!(info_label(&menu[0]), "G703");
        assert_eq!(info_label(&menu[1]), "42 % · conectado · perfil Trabalho");
        assert_eq!(radio_selected(&menu[3]), 0);
        assert_eq!(radio_selected(&menu[4]), 1);
    }

    fn answering_tray() -> (GhubTray, thread::JoinHandle<Vec<(Action, bool)>>) {
        let (tx, rx) = channel();
        let handle = answering_actions(rx, settled_view());
        (GhubTray::new(full_view(), tx), handle)
    }

    #[test]
    fn dpi_click_shows_the_replied_view() {
        let (mut t, actions) = answering_tray();
        select_radio(&mut t, 3, 0);
        assert_shows_settled_view(&t);
        drop(t);
        assert_eq!(actions.join().unwrap(), vec![(Action::SetDpiLevel(0), true)]);
    }

    #[test]
    fn profile_click_shows_the_replied_view() {
        let (mut t, actions) = answering_tray();
        select_radio(&mut t, 4, 1);
        assert_shows_settled_view(&t);
        drop(t);
        assert_eq!(
            actions.join().unwrap(),
            vec![(Action::ActivateProfile("Trabalho".into()), true)]
        );
    }

    #[test]
    fn autostart_click_shows_the_replied_view() {
        let (mut t, actions) = answering_tray();
        activate_checkmark(&mut t, 6);
        assert_shows_settled_view(&t);
        match &t.menu()[6] {
            MenuItem::Checkmark(c) => assert!(!c.checked),
            _ => panic!("esperava item marcável"),
        }
        drop(t);
        assert_eq!(actions.join().unwrap(), vec![(Action::ToggleAutostart, true)]);
    }

    #[test]
    fn silent_actions_side_keeps_the_optimistic_state_after_the_timeout() {
        let (tx, rx) = channel();
        let mut t = GhubTray::new(full_view(), tx).with_settle_timeout(Duration::from_millis(30));
        let start = Instant::now();
        select_radio(&mut t, 3, 0);
        assert!(start.elapsed() < Duration::from_millis(500));
        let req = rx.try_recv().unwrap();
        assert_eq!(req.action, Action::SetDpiLevel(0));
        assert!(req.reply.is_some());
        let menu = t.menu();
        assert_eq!(radio_selected(&menu[3]), 0);
        assert_eq!(info_label(&menu[0]), "G703");
        assert_eq!(t.tool_tip().description, "91 %");
    }

    #[test]
    fn gone_actions_side_returns_at_once_keeping_the_optimistic_state() {
        let (tx, rx) = channel();
        drop(rx);
        // Prazo longo: se esperasse por ele, o teste demoraria 10 s.
        let mut t = GhubTray::new(full_view(), tx).with_settle_timeout(Duration::from_secs(10));
        let start = Instant::now();
        select_radio(&mut t, 3, 0);
        select_radio(&mut t, 4, 1);
        activate_checkmark(&mut t, 6);
        assert!(start.elapsed() < Duration::from_millis(500));
        let menu = t.menu();
        assert_eq!(radio_selected(&menu[3]), 0);
        assert_eq!(radio_selected(&menu[4]), 1);
        match &menu[6] {
            MenuItem::Checkmark(c) => assert!(!c.checked),
            _ => panic!("esperava item marcável"),
        }
    }

    #[test]
    fn open_gui_quit_and_left_click_do_not_wait_for_a_view() {
        let (tx, rx) = channel();
        // Ninguém responde e o prazo é longo: esses itens não podem esperar.
        let mut t = GhubTray::new(full_view(), tx).with_settle_timeout(Duration::from_secs(10));
        let before = t.view.clone();
        let start = Instant::now();
        let menu = t.menu();
        if let MenuItem::Standard(i) = &menu[7] {
            (i.activate)(&mut t);
        }
        if let MenuItem::Standard(i) = &menu[8] {
            (i.activate)(&mut t);
        }
        t.activate(0, 0);
        assert!(start.elapsed() < Duration::from_millis(500));
        let got: Vec<(Action, bool)> =
            rx.try_iter().map(|r| (r.action, r.reply.is_some())).collect();
        assert_eq!(
            got,
            vec![(Action::OpenGui, false), (Action::Quit, false), (Action::OpenGui, false)]
        );
        assert_eq!(t.view, before);
    }

    /// Valida o marcador de layout (último item da raiz: submenu oculto, desabilitado e sem
    /// rótulo) e devolve quantos filhos ele tem agora.
    fn marker_children(t: &GhubTray) -> usize {
        let menu = t.menu();
        match menu.last() {
            Some(MenuItem::SubMenu(s)) => {
                assert!(!s.visible);
                assert!(!s.enabled);
                assert_eq!(s.label, "");
                for child in &s.submenu {
                    match child {
                        MenuItem::Standard(i) => {
                            assert!(!i.visible);
                            assert!(!i.enabled);
                            assert_eq!(i.label, "");
                        }
                        _ => panic!("esperava filho oculto simples no marcador"),
                    }
                }
                s.submenu.len()
            }
            _ => panic!("esperava o marcador de layout no fim do menu"),
        }
    }

    #[test]
    fn marker_is_the_last_item_and_never_leaks_elsewhere() {
        let (t, _rx) = tray(full_view());
        assert_eq!(marker_children(&t), 0);
        assert_eq!(t.menu().len(), full_view().entries.len() + 1);
        let tip = t.tool_tip();
        assert_eq!(tip.title, "G703");
        assert_eq!(tip.description, "91 %");
        assert_eq!(t.title(), "G HUB Linux");

        let (t, _rx) = tray(view(vec![]));
        assert_eq!(t.menu().len(), 1);
        assert_eq!(marker_children(&t), 0);
    }

    #[test]
    fn set_view_flips_the_marker_only_when_the_view_changes() {
        let (mut t, _rx) = tray(full_view());
        assert_eq!(marker_children(&t), 0);
        t.set_view(settled_view());
        assert_eq!(marker_children(&t), 1);
        t.set_view(settled_view());
        assert_eq!(marker_children(&t), 1);
        t.set_view(full_view());
        assert_eq!(marker_children(&t), 0);
        t.set_view(full_view());
        assert_eq!(marker_children(&t), 0);
    }

    #[test]
    fn settled_click_flips_the_marker_exactly_once() {
        let (mut t, actions) = answering_tray();
        // Otimista + vista respondida mudam a vista duas vezes, mas o marcador vira uma só
        // (duas viradas voltariam a 0 filhos).
        activate_checkmark(&mut t, 6);
        assert_shows_settled_view(&t);
        assert_eq!(marker_children(&t), 1);
        drop(t);
        assert_eq!(actions.join().unwrap(), vec![(Action::ToggleAutostart, true)]);
    }

    #[test]
    fn timed_out_click_with_an_optimistic_change_flips_the_marker_once() {
        let (mut t, rx) = tray(full_view());
        activate_checkmark(&mut t, 6);
        assert_eq!(rx.try_recv().unwrap().action, Action::ToggleAutostart);
        assert_eq!(marker_children(&t), 1);
        select_radio(&mut t, 3, 0);
        assert_eq!(rx.try_recv().unwrap().action, Action::SetDpiLevel(0));
        assert_eq!(marker_children(&t), 0);
    }

    #[test]
    fn click_whose_reply_restores_the_start_view_does_not_flip_the_marker() {
        let (tx, rx) = channel();
        let actions = answering_actions(rx, full_view());
        let mut t = GhubTray::new(full_view(), tx);
        activate_checkmark(&mut t, 6);
        assert_eq!(t.view, full_view());
        assert_eq!(marker_children(&t), 0);
        drop(t);
        assert_eq!(actions.join().unwrap(), vec![(Action::ToggleAutostart, true)]);
    }

    #[test]
    fn clicks_that_leave_the_view_unchanged_do_not_flip_the_marker() {
        let (mut t, _rx) = tray(full_view());
        let menu = t.menu();
        if let MenuItem::Standard(i) = &menu[7] {
            (i.activate)(&mut t);
        }
        if let MenuItem::Standard(i) = &menu[8] {
            (i.activate)(&mut t);
        }
        t.activate(0, 0);
        assert_eq!(marker_children(&t), 0);

        let (mut t, _rx) = tray(view(vec![Entry::Autostart { checked: true, enabled: false }]));
        activate_checkmark(&mut t, 0);
        assert_eq!(marker_children(&t), 0);

        let (mut t, _rx) =
            tray(view(vec![Entry::Profiles { labels: vec!["Padrão".into()], selected: 0 }]));
        select_radio(&mut t, 0, 5);
        assert_eq!(marker_children(&t), 0);
    }
}
