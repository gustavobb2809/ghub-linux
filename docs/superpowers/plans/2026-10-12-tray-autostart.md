# Bandeja, notificações e autostart — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dar ao `ghubd` um ícone na bandeja do sistema (pinguim, com estados), um menu para trocar DPI e perfil, notificação de bateria baixa e a opção "Iniciar com o sistema" (liga/desliga o `ghubd.service` do usuário), mais ativação sob demanda por D-Bus.

**Architecture:** A bandeja vive dentro do processo `ghubd` (crate `ksni`, StatusNotifierItem, sem Qt), então funciona com a GUI fechada. A lógica fica em módulos puros e testáveis: um rasterizador do ícone, um modelo que transforma o estado do daemon numa descrição de menu, um executor de ações (com `trait`s para o gerente, o systemd, o lançador da GUI e as notificações) e um "hub" com três laços (distribuir eventos, atualizar a bandeja com coalescência, executar ações). A cola com o `ksni` é fina. Sem tokio.

**Tech Stack:** Rust 2021; `ksni` 0.3.6 (`default-features = false`, features `blocking` e `async-io`); `zbus` 5 e `zvariant` 5 (notificações por `org.freedesktop.Notifications`). Validado num protótipo descartável: o item registra no `StatusNotifierWatcher` do Quickshell do usuário, o ícone por pixmap aparece e o menu (submenu, rádio, marcação, sair) abre.

**Spec:** `docs/superpowers/specs/2026-10-12-ghub-linux-design.md` (seção 5.3: bandeja, "Iniciar com o sistema", bateria baixa). API D-Bus e limitações em `docs/daemon.md`. Este é o **Plano 3 de 4**.

## Global Constraints

- Rust edição 2021; critério de qualidade: `RUSTFLAGS="-D warnings" cargo test --workspace` (clippy e rustfmt não estão instalados; não instalar).
- Nenhum `sudo`. Alterações no ambiente do usuário (instalar em `~/.local`, `~/.config/systemd/user`, `systemctl --user enable`) só na Task 9, com autorização explícita; o script de instalação só escreve sob `$HOME`.
- A bandeja vive no `ghubd` (spec 5.3). Sem tokio; `ksni` com `default-features = false, features = ["blocking", "async-io"]`.
- O daemon **nunca** deve falhar por causa da bandeja: sem `StatusNotifierWatcher` (login antes do shell) a bandeja espera (`assume_sni_available(true)`); se `spawn` falhar, o daemon continua e as notificações continuam.
- Textos da interface em português. Ícone simbólico na cor `#ebdbb2` (Gruvbox); selos: vermelho `#fb4934` (bateria baixa), verde `#b8bb26` (carregando); desconectado = 40 % de opacidade.
- Limiar de bateria baixa fixo nesta versão: notifica em ≤ 15 %, rearma em ≥ 20 % (já implementado no gerente); estado de carga desconhecido conta como descarregando.
- **Decisão do usuário (provisória):** o perfil continua sendo reaplicado quando o mouse volta/acorda (comportamento atual do gerente). Não alterar isso neste plano.
- Testes unitários não dependem de hardware, do barramento D-Bus de sessão, de `systemctl` real nem de um servidor de notificações.

## Review Focus

Condições que a spec implica, mas que testes óbvios não cobririam, da mais provável para a menos:

1. Daemon sobe antes do shell/bandeja (login) ou sem bandeja nenhuma: não pode morrer nem travar, e as notificações de bateria continuam (Tasks 6 e 7).
2. Mouse dormindo/desligado: o ícone fica "desconectado" mas mostra a última bateria conhecida e **não** pisca a cada ciclo; menu de DPI desabilitado (Task 2).
3. Ação disparada pela bandeja sem dispositivo, com dispositivo offline ou com erro do gerente: vira notificação de erro, nunca pânico nem escrita indevida (Task 5).
4. "Iniciar com o sistema" quando o serviço do systemd não está instalado: item desabilitado, mensagem clara, nunca roda `systemctl enable` num serviço inexistente; falhas do `systemctl` aparecem ao usuário (Tasks 4 e 5).
5. "Sair e parar o serviço" tanto sob o systemd quanto com o daemon iniciado à mão; não deixa o processo meio vivo nem dispara `Restart=on-failure` (Tasks 4, 5 e 7).
6. Rajada de eventos (varreduras, bateria) não deve causar uma enxurrada de reconstruções do menu (coalescência) nem travar o gerente (Task 7).

---

### Task 1: Rasterizador do ícone (`tray_icon`)

**Files:**
- Modify: `crates/ghubd/Cargo.toml` (dependências novas)
- Create: `crates/ghubd/src/tray_icon.rs`
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Produces (`ghubd::tray_icon`):
  - `IconState { Normal, LowBattery, Charging, Disconnected }` (`Debug, Clone, Copy, PartialEq, Eq`).
  - `Pixmap { size: u32, argb: Vec<u8> }` com `size * size * 4` bytes por imagem, na ordem A, R, G, B (rede) por pixel.
  - `SIZES: [u32; 3] = [22, 32, 48]`.
  - `render(state: IconState, size: u32) -> Pixmap`.

- [ ] **Step 1: Dependências**

Em `crates/ghubd/Cargo.toml`, na seção `[dependencies]`, acrescentar:

```toml
ksni = { version = "0.3.6", default-features = false, features = ["blocking", "async-io"] }
zvariant = "5"
```

- [ ] **Step 2: Escrever os testes que falham**

`crates/ghubd/src/tray_icon.rs` (só os testes e as declarações mínimas):

```rust
//! Ícone simbólico do pinguim, rasterizado em código (ARGB32, ordem de rede).

#[cfg(test)]
mod tests {
    use super::*;

    fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.size + x) * 4) as usize;
        [p.argb[i], p.argb[i + 1], p.argb[i + 2], p.argb[i + 3]]
    }

    #[test]
    fn has_the_expected_buffer_size_for_every_size() {
        for size in SIZES {
            for state in [
                IconState::Normal,
                IconState::LowBattery,
                IconState::Charging,
                IconState::Disconnected,
            ] {
                let p = render(state, size);
                assert_eq!(p.size, size);
                assert_eq!(p.argb.len(), (size * size * 4) as usize);
            }
        }
    }

    #[test]
    fn corners_are_transparent() {
        let p = render(IconState::Normal, 48);
        assert_eq!(px(&p, 0, 0)[0], 0);
        assert_eq!(px(&p, 47, 0)[0], 0);
        assert_eq!(px(&p, 0, 47)[0], 0);
    }

    #[test]
    fn head_is_opaque_glyph_color() {
        let p = render(IconState::Normal, 48);
        assert_eq!(px(&p, 24, 12), [255, 0xeb, 0xdb, 0xb2]);
    }

    #[test]
    fn belly_is_cut_out() {
        let p = render(IconState::Normal, 48);
        assert_eq!(px(&p, 24, 31)[0], 0);
    }

    #[test]
    fn low_battery_has_a_red_badge_and_charging_a_green_one() {
        let low = render(IconState::LowBattery, 48);
        let charging = render(IconState::Charging, 48);
        let normal = render(IconState::Normal, 48);
        assert_eq!(px(&low, 37, 37), [255, 0xfb, 0x49, 0x34]);
        assert_eq!(px(&charging, 37, 37), [255, 0xb8, 0xbb, 0x26]);
        assert_ne!(px(&normal, 37, 37), px(&low, 37, 37));
    }

    #[test]
    fn disconnected_is_dimmer_but_visible() {
        let normal = render(IconState::Normal, 48);
        let off = render(IconState::Disconnected, 48);
        assert_eq!(px(&normal, 24, 12)[0], 255);
        let alpha = px(&off, 24, 12)[0];
        assert!((90..=110).contains(&alpha), "alpha = {alpha}");
    }

    #[test]
    fn rendering_is_deterministic_and_states_differ() {
        assert_eq!(render(IconState::Normal, 22), render(IconState::Normal, 22));
        let all = [
            render(IconState::Normal, 22),
            render(IconState::LowBattery, 22),
            render(IconState::Charging, 22),
            render(IconState::Disconnected, 22),
        ];
        for i in 0..all.len() {
            for j in (i + 1)..all.len() {
                assert_ne!(all[i], all[j], "{i} vs {j}");
            }
        }
    }
}
```

Em `lib.rs` acrescentar `pub mod tray_icon;`.

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p ghubd tray_icon`
Expected: FAIL de compilação (`Pixmap`, `render`… não definidos).

- [ ] **Step 4: Implementar**

Inserir em `tray_icon.rs`, acima do bloco de testes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconState {
    Normal,
    LowBattery,
    Charging,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixmap {
    pub size: u32,
    /// `size * size` pixels, cada um em 4 bytes: A, R, G, B.
    pub argb: Vec<u8>,
}

pub const SIZES: [u32; 3] = [22, 32, 48];

const GLYPH: [u8; 3] = [0xeb, 0xdb, 0xb2];
const RED: [u8; 3] = [0xfb, 0x49, 0x34];
const GREEN: [u8; 3] = [0xb8, 0xbb, 0x26];
/// Subamostras por eixo (4x4 = 16 por pixel) para suavizar as bordas.
const SS: u32 = 4;

fn in_ellipse(x: f32, y: f32, cx: f32, cy: f32, rx: f32, ry: f32) -> bool {
    let dx = (x - cx) / rx;
    let dy = (y - cy) / ry;
    dx * dx + dy * dy <= 1.0
}

fn in_circle(x: f32, y: f32, cx: f32, cy: f32, r: f32) -> bool {
    in_ellipse(x, y, cx, cy, r, r)
}

/// O pinguim num quadrado unitário: corpo, asas e pés, com barriga e olhos vazados.
fn glyph_at(x: f32, y: f32) -> bool {
    let shape = in_ellipse(x, y, 0.5, 0.55, 0.26, 0.36)
        || in_ellipse(x, y, 0.235, 0.62, 0.065, 0.20)
        || in_ellipse(x, y, 0.765, 0.62, 0.065, 0.20)
        || in_ellipse(x, y, 0.38, 0.93, 0.09, 0.045)
        || in_ellipse(x, y, 0.62, 0.93, 0.09, 0.045);
    let cut = in_ellipse(x, y, 0.5, 0.64, 0.15, 0.24)
        || in_circle(x, y, 0.43, 0.36, 0.045)
        || in_circle(x, y, 0.57, 0.36, 0.045);
    shape && !cut
}

fn sample(state: IconState, x: f32, y: f32) -> Option<[u8; 3]> {
    match state {
        IconState::LowBattery if in_circle(x, y, 0.78, 0.78, 0.2) => return Some(RED),
        IconState::Charging if in_circle(x, y, 0.78, 0.78, 0.2) => return Some(GREEN),
        _ => {}
    }
    glyph_at(x, y).then_some(GLYPH)
}

pub fn render(state: IconState, size: u32) -> Pixmap {
    let mut argb = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let (mut covered, mut r, mut g, mut b) = (0u32, 0u32, 0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    if let Some(c) = sample(state, x, y) {
                        covered += 1;
                        r += u32::from(c[0]);
                        g += u32::from(c[1]);
                        b += u32::from(c[2]);
                    }
                }
            }
            if covered == 0 {
                argb.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let mut alpha = covered * 255 / (SS * SS);
            if state == IconState::Disconnected {
                alpha = alpha * 2 / 5;
            }
            argb.extend_from_slice(&[
                alpha as u8,
                (r / covered) as u8,
                (g / covered) as u8,
                (b / covered) as u8,
            ]);
        }
    }
    Pixmap { size, argb }
}
```

- [ ] **Step 5: Rodar e ver passar**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos (baixar `ksni` e dependências é esperado).

- [ ] **Step 6: Commit**

```bash
git add Cargo.lock crates/ghubd
git commit -m "feat(ghubd): add tray icon rasterizer and ksni/zvariant dependencies"
```

---

### Task 2: Modelo do menu da bandeja (`tray_model`)

**Files:**
- Create: `crates/ghubd/src/tray_model.rs`
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Consumes: `ghub_ipc::{DeviceState, DeviceSummary, STATE_CHARGING, STATE_FULL}`, `manager::LOW_BATTERY_PERCENT`, `tray_icon::IconState`.
- Produces (`ghubd::tray_model`):
  - `APP_NAME = "G HUB Linux"`.
  - `Autostart { Enabled, Disabled, Unavailable }` (`Debug, Clone, Copy, PartialEq, Eq`).
  - `TrayInput { devices: Vec<DeviceSummary>, primary: Option<DeviceState>, profiles: Vec<String>, active_profile: String, autostart: Autostart }` com `TrayInput::empty()`.
  - `Entry { Info(String), Separator, Dpi { labels: Vec<String>, selected: usize, enabled: bool }, Profiles { labels: Vec<String>, selected: usize }, Autostart { checked: bool, enabled: bool }, OpenGui, Quit }`.
  - `TrayView { icon: IconState, title: String, tooltip_title: String, tooltip_text: String, entries: Vec<Entry> }`.
  - `pick_primary(devices: &[DeviceSummary]) -> Option<&DeviceSummary>` e `build_view(input: &TrayInput) -> TrayView`.

- [ ] **Step 1: Escrever os testes que falham**

`crates/ghubd/src/tray_model.rs` (só `use` e testes):

```rust
use ghub_ipc::{DeviceState, DeviceSummary, STATE_CHARGING, STATE_FULL};

use crate::manager::LOW_BATTERY_PERCENT;
use crate::tray_icon::IconState;

#[cfg(test)]
mod tests {
    use super::*;

    fn state(connected: bool, percent: i32, battery_state: &str) -> DeviceState {
        DeviceState {
            id: "046dc539-1".into(),
            name: "G703".into(),
            connected,
            dpi_current: if connected { 1200 } else { 0 },
            dpi_levels: vec![800, 1200, 1600],
            active_level: 1,
            dpi_min: 100,
            dpi_max: 25600,
            dpi_step: 50,
            dpi_list: vec![],
            polling_hz: if connected { 1000 } else { 0 },
            polling_supported: vec![125, 250, 500, 1000],
            battery_percent: percent,
            battery_state: battery_state.into(),
            profile: "Padrão".into(),
        }
    }

    fn summary(id: &str, connected: bool) -> DeviceSummary {
        DeviceSummary {
            id: id.into(),
            name: id.into(),
            connected,
            battery_percent: 50,
            battery_state: "discharging".into(),
        }
    }

    fn input(primary: Option<DeviceState>) -> TrayInput {
        TrayInput {
            devices: vec![],
            primary,
            profiles: vec!["Padrão".into(), "Trabalho".into()],
            active_profile: "Trabalho".into(),
            autostart: Autostart::Disabled,
        }
    }

    #[test]
    fn connected_device_builds_the_full_menu() {
        let v = build_view(&input(Some(state(true, 91, "discharging"))));
        assert_eq!(v.icon, IconState::Normal);
        assert_eq!(v.title, APP_NAME);
        assert_eq!(v.tooltip_title, "G703");
        assert_eq!(v.tooltip_text, "91 % · 1200 DPI");
        assert_eq!(
            v.entries,
            vec![
                Entry::Info("G703 — 91 %".into()),
                Entry::Info("1200 DPI · 1000 Hz".into()),
                Entry::Separator,
                Entry::Dpi {
                    labels: vec!["800 DPI".into(), "1200 DPI".into(), "1600 DPI".into()],
                    selected: 1,
                    enabled: true
                },
                Entry::Separator,
                Entry::Profiles {
                    labels: vec!["Padrão".into(), "Trabalho".into()],
                    selected: 1
                },
                Entry::Separator,
                Entry::Autostart { checked: false, enabled: true },
                Entry::OpenGui,
                Entry::Quit,
            ]
        );
    }

    #[test]
    fn icon_follows_the_battery() {
        let icon = |p, s| build_view(&input(Some(state(true, p, s)))).icon;
        assert_eq!(icon(15, "discharging"), IconState::LowBattery);
        assert_eq!(icon(16, "discharging"), IconState::Normal);
        assert_eq!(icon(5, "unknown"), IconState::LowBattery);
        assert_eq!(icon(5, STATE_CHARGING), IconState::Charging);
        assert_eq!(icon(5, STATE_FULL), IconState::Normal);
        assert_eq!(icon(-1, "unknown"), IconState::Normal);
        assert_eq!(icon(80, STATE_CHARGING), IconState::Charging);
    }

    #[test]
    fn charging_text_and_unknown_battery_text() {
        let v = build_view(&input(Some(state(true, 80, STATE_CHARGING))));
        assert_eq!(v.tooltip_text, "80 % (carregando) · 1200 DPI");
        let v = build_view(&input(Some(state(true, -1, "unknown"))));
        assert_eq!(v.tooltip_text, "bateria desconhecida · 1200 DPI");
        let v = build_view(&input(Some(state(true, 100, STATE_FULL))));
        assert_eq!(v.tooltip_text, "100 % (carga completa) · 1200 DPI");
    }

    #[test]
    fn disconnected_device_keeps_the_last_battery_and_disables_dpi() {
        let v = build_view(&input(Some(state(false, 89, "unknown"))));
        assert_eq!(v.icon, IconState::Disconnected);
        assert_eq!(v.tooltip_text, "Inativo ou dormindo · 89 %");
        assert_eq!(v.entries[0], Entry::Info("G703 — 89 %".into()));
        assert_eq!(v.entries[1], Entry::Info("Inativo ou dormindo".into()));
        assert!(v.entries.iter().any(
            |e| matches!(e, Entry::Dpi { enabled: false, .. })
        ));
    }

    #[test]
    fn disconnected_with_low_battery_is_still_disconnected() {
        let v = build_view(&input(Some(state(false, 3, "unknown"))));
        assert_eq!(v.icon, IconState::Disconnected);
    }

    #[test]
    fn no_device_shows_a_clear_message_and_no_dpi_entry() {
        let v = build_view(&input(None));
        assert_eq!(v.icon, IconState::Disconnected);
        assert_eq!(v.tooltip_title, APP_NAME);
        assert_eq!(v.tooltip_text, "Nenhum dispositivo encontrado");
        assert_eq!(v.entries[0], Entry::Info("Nenhum dispositivo encontrado".into()));
        assert!(!v.entries.iter().any(|e| matches!(e, Entry::Dpi { .. })));
        assert!(v.entries.contains(&Entry::OpenGui));
        assert!(v.entries.contains(&Entry::Quit));
    }

    #[test]
    fn profile_selection_and_absence() {
        let mut i = input(None);
        i.active_profile = "não existe".into();
        let v = build_view(&i);
        assert!(v.entries.iter().any(
            |e| matches!(e, Entry::Profiles { selected: 0, .. })
        ));
        i.profiles.clear();
        let v = build_view(&i);
        assert!(!v.entries.iter().any(|e| matches!(e, Entry::Profiles { .. })));
        // nunca dois separadores seguidos
        for pair in v.entries.windows(2) {
            assert!(!(pair[0] == Entry::Separator && pair[1] == Entry::Separator));
        }
    }

    #[test]
    fn autostart_states() {
        let entry = |a| {
            let mut i = input(None);
            i.autostart = a;
            build_view(&i)
                .entries
                .into_iter()
                .find(|e| matches!(e, Entry::Autostart { .. }))
                .unwrap()
        };
        assert_eq!(entry(Autostart::Enabled), Entry::Autostart { checked: true, enabled: true });
        assert_eq!(entry(Autostart::Disabled), Entry::Autostart { checked: false, enabled: true });
        assert_eq!(entry(Autostart::Unavailable), Entry::Autostart { checked: false, enabled: false });
    }

    #[test]
    fn active_level_out_of_range_is_clamped_and_empty_levels_hide_the_entry() {
        let mut s = state(true, 50, "discharging");
        s.active_level = 9;
        let v = build_view(&input(Some(s.clone())));
        assert!(v.entries.iter().any(
            |e| matches!(e, Entry::Dpi { selected: 2, .. })
        ));
        s.dpi_levels.clear();
        let v = build_view(&input(Some(s)));
        assert!(!v.entries.iter().any(|e| matches!(e, Entry::Dpi { .. })));
    }

    #[test]
    fn primary_prefers_a_connected_device() {
        assert_eq!(pick_primary(&[]), None);
        let ds = vec![summary("a", false), summary("b", true)];
        assert_eq!(pick_primary(&ds).unwrap().id, "b");
        let ds = vec![summary("a", false), summary("c", false)];
        assert_eq!(pick_primary(&ds).unwrap().id, "a");
    }
}
```

Em `lib.rs` acrescentar `pub mod tray_model;`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p ghubd tray_model`
Expected: FAIL de compilação (`TrayInput`, `build_view`… não definidos).

- [ ] **Step 3: Implementar**

Inserir em `tray_model.rs`, entre os `use` e os testes:

```rust
pub const APP_NAME: &str = "G HUB Linux";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autostart {
    Enabled,
    Disabled,
    /// O serviço do systemd não está instalado (ou o `systemctl` não responde).
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayInput {
    pub devices: Vec<DeviceSummary>,
    /// Estado do dispositivo principal (veja `pick_primary`), se houver.
    pub primary: Option<DeviceState>,
    pub profiles: Vec<String>,
    pub active_profile: String,
    pub autostart: Autostart,
}

impl TrayInput {
    pub fn empty() -> Self {
        TrayInput {
            devices: Vec::new(),
            primary: None,
            profiles: Vec::new(),
            active_profile: String::new(),
            autostart: Autostart::Unavailable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// Linha informativa (desabilitada).
    Info(String),
    Separator,
    Dpi { labels: Vec<String>, selected: usize, enabled: bool },
    Profiles { labels: Vec<String>, selected: usize },
    Autostart { checked: bool, enabled: bool },
    OpenGui,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    pub icon: IconState,
    pub title: String,
    pub tooltip_title: String,
    pub tooltip_text: String,
    pub entries: Vec<Entry>,
}

/// O dispositivo que a bandeja mostra: o primeiro conectado, senão o primeiro da lista.
pub fn pick_primary(devices: &[DeviceSummary]) -> Option<&DeviceSummary> {
    devices.iter().find(|d| d.connected).or_else(|| devices.first())
}

fn battery_text(percent: i32, state: &str) -> String {
    if percent < 0 {
        return "bateria desconhecida".to_string();
    }
    let suffix = match state {
        STATE_CHARGING => " (carregando)",
        STATE_FULL => " (carga completa)",
        _ => "",
    };
    format!("{percent} %{suffix}")
}

fn icon_state(device: Option<&DeviceState>) -> IconState {
    match device {
        None => IconState::Disconnected,
        Some(d) if !d.connected => IconState::Disconnected,
        Some(d) if d.battery_state == STATE_CHARGING => IconState::Charging,
        Some(d)
            if d.battery_percent >= 0
                && d.battery_percent <= i32::from(LOW_BATTERY_PERCENT)
                && d.battery_state != STATE_FULL =>
        {
            IconState::LowBattery
        }
        Some(_) => IconState::Normal,
    }
}

pub fn build_view(input: &TrayInput) -> TrayView {
    let device = input.primary.as_ref();
    let icon = icon_state(device);
    let mut entries = Vec::new();

    let (tooltip_title, tooltip_text) = match device {
        None => {
            entries.push(Entry::Info("Nenhum dispositivo encontrado".into()));
            (APP_NAME.to_string(), "Nenhum dispositivo encontrado".to_string())
        }
        Some(d) => {
            let battery = battery_text(d.battery_percent, &d.battery_state);
            entries.push(Entry::Info(format!("{} — {}", d.name, battery)));
            let text = if d.connected {
                entries.push(Entry::Info(format!(
                    "{} DPI · {} Hz",
                    d.dpi_current, d.polling_hz
                )));
                format!("{battery} · {} DPI", d.dpi_current)
            } else {
                entries.push(Entry::Info("Inativo ou dormindo".into()));
                format!("Inativo ou dormindo · {battery}")
            };
            if !d.dpi_levels.is_empty() {
                entries.push(Entry::Separator);
                entries.push(Entry::Dpi {
                    labels: d.dpi_levels.iter().map(|v| format!("{v} DPI")).collect(),
                    selected: (d.active_level as usize).min(d.dpi_levels.len() - 1),
                    enabled: d.connected,
                });
            }
            (d.name.clone(), text)
        }
    };

    if !input.profiles.is_empty() {
        entries.push(Entry::Separator);
        let selected = input
            .profiles
            .iter()
            .position(|p| *p == input.active_profile)
            .unwrap_or(0);
        entries.push(Entry::Profiles { labels: input.profiles.clone(), selected });
    }

    entries.push(Entry::Separator);
    entries.push(Entry::Autostart {
        checked: input.autostart == Autostart::Enabled,
        enabled: input.autostart != Autostart::Unavailable,
    });
    entries.push(Entry::OpenGui);
    entries.push(Entry::Quit);

    TrayView {
        icon,
        title: APP_NAME.to_string(),
        tooltip_title,
        tooltip_text,
        entries,
    }
}
```

Observação: no teste `no_device_shows_a_clear_message...`, sem dispositivo o menu é `Info`, (`Separator` + `Profiles` se houver perfis), `Separator`, `Autostart`, `OpenGui`, `Quit`; se o teste de "dois separadores seguidos" falhar, ajustar o **código** para nunca emitir dois separadores seguidos (por exemplo, não emitir o separador antes de `Profiles` quando a última entrada já for `Separator`).

- [ ] **Step 4: Rodar tudo**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos.

- [ ] **Step 5: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): add tray view model (icon state, tooltip and menu entries)"
```

---

### Task 3: Notificações (`notify`)

**Files:**
- Create: `crates/ghubd/src/notify.rs`
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Produces (`ghubd::notify`):
  - `Urgency { Low = 0, Normal = 1, Critical = 2 }` (`#[repr(u8)]`, `Debug, Clone, Copy, PartialEq, Eq`).
  - `Notification { summary: String, body: String, urgency: Urgency, icon: String }` com `battery_low(device_name: &str, percent: u8)`, `error(text: &str)`, `test()`.
  - `trait Notifier: Send + Sync { fn notify(&self, n: &Notification); }`.
  - `DbusNotifier::new() -> Result<DbusNotifier, zbus::Error>` (usa o barramento de sessão e `org.freedesktop.Notifications.Notify`) e `LogNotifier` (escreve em `stderr`).

- [ ] **Step 1: Escrever os testes que falham**

`crates/ghubd/src/notify.rs`:

```rust
use std::collections::HashMap;

use zbus::blocking::Connection;
use zvariant::Value;

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
```

Em `lib.rs` acrescentar `pub mod notify;`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p ghubd notify`
Expected: FAIL de compilação (`Notification`, `Urgency`, `LogNotifier` não definidos).

- [ ] **Step 3: Implementar**

Inserir em `notify.rs`, entre os `use` e os testes:

```rust
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
```

- [ ] **Step 4: Rodar tudo**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos. (`DbusNotifier` é verificado só por compilação aqui; a Task 9 o exercita de verdade com `ghubd --notify-test`.)

- [ ] **Step 5: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): add desktop notifications (message builders and D-Bus notifier)"
```

---

### Task 4: Ligações com o gerente, o systemd e a GUI (`link`)

**Files:**
- Create: `crates/ghubd/src/link.rs`
- Modify: `crates/ghubd/src/testing.rs` (dublês, só `cfg(test)`)
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Consumes: `manager::{Command, ManagerError}`, `tray_model::Autostart`, `ghub_ipc::{DeviceState, DeviceSummary}`, `notify::{Notification, Notifier}`.
- Produces (`ghubd::link`):
  - `trait ManagerLink: Send + Sync { devices() -> Vec<DeviceSummary>; state(&str) -> Option<DeviceState>; profiles() -> Vec<String>; active_profile() -> String; set_active_level(&str, usize) -> Result<(), ManagerError>; activate_profile(&str) -> Result<(), ManagerError>; shutdown(); }`.
  - `ChannelLink::new(tx: Sender<Command>) -> ChannelLink` (implementa `ManagerLink`; respostas com tempo limite de 5 s; gerente parado devolve valores vazios/`Err`).
  - `trait ServiceControl: Send + Sync { autostart() -> Autostart; set_autostart(bool) -> Result<(), String>; stop() -> Result<bool, String>; }`, `SystemctlControl`, `UNIT = "ghubd.service"`, `parse_is_enabled(&str) -> Autostart`, `runs_under_systemd(Option<&str>) -> bool`.
  - `trait Launcher: Send + Sync { open_gui() -> Result<(), String>; }` e `GuiLauncher` (executa `ghub-gui`).
  - Em `testing`: `FakeLink`, `FakeService`, `FakeLauncher`, `FakeNotifier`, `sample_state(...)`, `sample_summary(...)`.

- [ ] **Step 1: Dublês de teste**

Em `crates/ghubd/src/testing.rs`, acrescentar ao final do arquivo (e os `use` necessários no topo: `use std::sync::Mutex` já existe via `Arc, Mutex, MutexGuard`; acrescentar os demais):

```rust
use ghub_ipc::{DeviceState, DeviceSummary};

use crate::link::{Launcher, ManagerLink, ServiceControl};
use crate::manager::ManagerError;
use crate::notify::{Notification, Notifier};
use crate::tray_model::Autostart;

pub fn sample_summary(id: &str, connected: bool) -> DeviceSummary {
    DeviceSummary {
        id: id.into(),
        name: format!("Mouse {id}"),
        connected,
        battery_percent: 80,
        battery_state: "discharging".into(),
    }
}

pub fn sample_state(id: &str, connected: bool) -> DeviceState {
    DeviceState {
        id: id.into(),
        name: format!("Mouse {id}"),
        connected,
        dpi_current: if connected { 1200 } else { 0 },
        dpi_levels: vec![800, 1200, 1600],
        active_level: 1,
        dpi_min: 100,
        dpi_max: 25600,
        dpi_step: 50,
        dpi_list: vec![],
        polling_hz: if connected { 1000 } else { 0 },
        polling_supported: vec![125, 250, 500, 1000],
        battery_percent: 80,
        battery_state: "discharging".into(),
        profile: "Padrão".into(),
    }
}

pub struct FakeLink {
    pub devices: Mutex<Vec<DeviceSummary>>,
    pub state: Mutex<Option<DeviceState>>,
    pub profiles: Mutex<Vec<String>>,
    pub active: Mutex<String>,
    pub level_result: Mutex<Result<(), ManagerError>>,
    pub profile_result: Mutex<Result<(), ManagerError>>,
    pub calls: Mutex<Vec<String>>,
}

impl Default for FakeLink {
    fn default() -> Self {
        FakeLink {
            devices: Mutex::new(Vec::new()),
            state: Mutex::new(None),
            profiles: Mutex::new(Vec::new()),
            active: Mutex::new(String::new()),
            level_result: Mutex::new(Ok(())),
            profile_result: Mutex::new(Ok(())),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl FakeLink {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl ManagerLink for FakeLink {
    fn devices(&self) -> Vec<DeviceSummary> {
        self.devices.lock().unwrap().clone()
    }
    fn state(&self, _id: &str) -> Option<DeviceState> {
        self.state.lock().unwrap().clone()
    }
    fn profiles(&self) -> Vec<String> {
        self.profiles.lock().unwrap().clone()
    }
    fn active_profile(&self) -> String {
        self.active.lock().unwrap().clone()
    }
    fn set_active_level(&self, id: &str, level: usize) -> Result<(), ManagerError> {
        self.calls.lock().unwrap().push(format!("set_active_level:{id}:{level}"));
        self.level_result.lock().unwrap().clone()
    }
    fn activate_profile(&self, name: &str) -> Result<(), ManagerError> {
        self.calls.lock().unwrap().push(format!("activate_profile:{name}"));
        self.profile_result.lock().unwrap().clone()
    }
    fn shutdown(&self) {
        self.calls.lock().unwrap().push("shutdown".into());
    }
}

pub struct FakeService {
    pub autostart: Mutex<Autostart>,
    pub set_result: Mutex<Result<(), String>>,
    pub stop_result: Mutex<Result<bool, String>>,
    pub calls: Mutex<Vec<String>>,
}

impl Default for FakeService {
    fn default() -> Self {
        FakeService {
            autostart: Mutex::new(Autostart::Disabled),
            set_result: Mutex::new(Ok(())),
            stop_result: Mutex::new(Ok(true)),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl FakeService {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl ServiceControl for FakeService {
    fn autostart(&self) -> Autostart {
        *self.autostart.lock().unwrap()
    }
    fn set_autostart(&self, enable: bool) -> Result<(), String> {
        self.calls.lock().unwrap().push(format!("set_autostart:{enable}"));
        self.set_result.lock().unwrap().clone()
    }
    fn stop(&self) -> Result<bool, String> {
        self.calls.lock().unwrap().push("stop".into());
        self.stop_result.lock().unwrap().clone()
    }
}

pub struct FakeLauncher {
    pub result: Mutex<Result<(), String>>,
    pub opened: Mutex<u32>,
}

impl Default for FakeLauncher {
    fn default() -> Self {
        FakeLauncher { result: Mutex::new(Ok(())), opened: Mutex::new(0) }
    }
}

impl Launcher for FakeLauncher {
    fn open_gui(&self) -> Result<(), String> {
        *self.opened.lock().unwrap() += 1;
        self.result.lock().unwrap().clone()
    }
}

#[derive(Default)]
pub struct FakeNotifier {
    pub sent: Mutex<Vec<Notification>>,
}

impl FakeNotifier {
    pub fn sent(&self) -> Vec<Notification> {
        self.sent.lock().unwrap().clone()
    }
}

impl Notifier for FakeNotifier {
    fn notify(&self, n: &Notification) {
        self.sent.lock().unwrap().push(n.clone());
    }
}
```

- [ ] **Step 2: Escrever os testes que falham (`link.rs`)**

`crates/ghubd/src/link.rs` (só `use` e testes):

```rust
use std::process::Command as Process;
use std::sync::mpsc::{channel, Sender};
use std::time::Duration;

use ghub_ipc::{DeviceState, DeviceSummary};

use crate::manager::{Command, ManagerError};
use crate::tray_model::Autostart;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::Manager;
    use crate::profile::ProfileStore;
    use crate::testing::{FakeBackend, FakeMouse};
    use std::thread;

    #[test]
    fn parses_systemctl_is_enabled_output() {
        assert_eq!(parse_is_enabled("enabled\n"), Autostart::Enabled);
        assert_eq!(parse_is_enabled("enabled-runtime\n"), Autostart::Enabled);
        assert_eq!(parse_is_enabled("disabled\n"), Autostart::Disabled);
        for other in ["", "static\n", "masked\n", "linked\n", "not-found\n", "garbage"] {
            assert_eq!(parse_is_enabled(other), Autostart::Unavailable, "{other:?}");
        }
    }

    #[test]
    fn detects_when_running_under_systemd() {
        assert!(runs_under_systemd(Some("abc123")));
        assert!(!runs_under_systemd(Some("")));
        assert!(!runs_under_systemd(None));
    }

    fn running_link() -> (ChannelLink, Sender<Command>, thread::JoinHandle<()>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::from_toml(
            "active = \"Padrão\"\n[[profile]]\nname = \"Padrão\"\ndpi_levels = [800, 1600]\nactive_level = 1\npolling_hz = 1000\n",
        )
        .unwrap();
        let (backend, _bat) = FakeBackend::with_mouse(FakeMouse::new(1600, 1000));
        let (ev_tx, _ev_rx) = channel();
        let manager = Manager::new(backend, store, dir.path().join("profiles.toml"), ev_tx);
        let (tx, rx) = channel();
        let handle = thread::spawn(move || manager.run(rx, Duration::from_secs(60)));
        (ChannelLink::new(tx.clone()), tx, handle, dir)
    }

    #[test]
    fn channel_link_reads_and_writes_through_the_manager() {
        let (link, tx, handle, _d) = running_link();
        let devices = link.devices();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "dev-1");
        assert_eq!(link.state("dev-1").unwrap().dpi_current, 1600);
        assert!(link.state("nope").is_none());
        assert_eq!(link.profiles(), vec!["Padrão".to_string()]);
        assert_eq!(link.active_profile(), "Padrão");
        link.set_active_level("dev-1", 0).unwrap();
        assert_eq!(link.state("dev-1").unwrap().dpi_current, 800);
        assert!(matches!(
            link.set_active_level("dev-1", 9),
            Err(ManagerError::Invalid(_))
        ));
        assert!(matches!(
            link.activate_profile("zzz"),
            Err(ManagerError::NotFound(_))
        ));
        link.activate_profile("Padrão").unwrap();
        link.shutdown();
        handle.join().unwrap();
        drop(tx);
    }

    #[test]
    fn channel_link_degrades_gracefully_when_the_manager_is_gone() {
        let (link, tx, handle, _d) = running_link();
        link.shutdown();
        handle.join().unwrap();
        drop(tx);
        assert!(link.devices().is_empty());
        assert!(link.state("dev-1").is_none());
        assert!(link.profiles().is_empty());
        assert_eq!(link.active_profile(), "");
        assert!(matches!(
            link.set_active_level("dev-1", 0),
            Err(ManagerError::Device(_))
        ));
    }

    #[test]
    fn channel_link_times_out_instead_of_hanging() {
        let (tx, _rx_kept_alive) = channel::<Command>();
        let link = ChannelLink::with_timeout(tx, Duration::from_millis(50));
        assert!(link.devices().is_empty());
        assert!(matches!(
            link.activate_profile("x"),
            Err(ManagerError::Device(_))
        ));
    }
}
```

Em `lib.rs` acrescentar `pub mod link;`.

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p ghubd link`
Expected: FAIL de compilação (`ChannelLink`, `parse_is_enabled`… não definidos; os dublês em `testing.rs` também não compilam ainda sem os `trait`s).

- [ ] **Step 4: Implementar**

Inserir em `link.rs`, entre os `use` e os testes:

```rust
/// Unidade do systemd de usuário que roda o daemon.
pub const UNIT: &str = "ghubd.service";
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// O que a bandeja precisa do gerente de dispositivos.
pub trait ManagerLink: Send + Sync {
    fn devices(&self) -> Vec<DeviceSummary>;
    fn state(&self, id: &str) -> Option<DeviceState>;
    fn profiles(&self) -> Vec<String>;
    fn active_profile(&self) -> String;
    fn set_active_level(&self, id: &str, level: usize) -> Result<(), ManagerError>;
    fn activate_profile(&self, name: &str) -> Result<(), ManagerError>;
    /// Pede ao gerente que pare (fim do laço principal).
    fn shutdown(&self);
}

/// Fala com o gerente pelos mesmos canais que o serviço D-Bus usa.
pub struct ChannelLink {
    tx: Sender<Command>,
    timeout: Duration,
}

impl ChannelLink {
    pub fn new(tx: Sender<Command>) -> Self {
        Self::with_timeout(tx, REPLY_TIMEOUT)
    }

    pub fn with_timeout(tx: Sender<Command>, timeout: Duration) -> Self {
        ChannelLink { tx, timeout }
    }

    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> Command) -> Option<T> {
        let (reply_tx, reply_rx) = channel();
        self.tx.send(make(reply_tx)).ok()?;
        reply_rx.recv_timeout(self.timeout).ok()
    }

    fn unreachable() -> ManagerError {
        ManagerError::Device("o gerente de dispositivos não respondeu".into())
    }
}

impl ManagerLink for ChannelLink {
    fn devices(&self) -> Vec<DeviceSummary> {
        self.ask(Command::Devices).unwrap_or_default()
    }

    fn state(&self, id: &str) -> Option<DeviceState> {
        let id = id.to_string();
        self.ask(|reply| Command::State { id, reply })?.ok()
    }

    fn profiles(&self) -> Vec<String> {
        self.ask(Command::ListProfiles).unwrap_or_default()
    }

    fn active_profile(&self) -> String {
        self.ask(Command::ActiveProfile).unwrap_or_default()
    }

    fn set_active_level(&self, id: &str, level: usize) -> Result<(), ManagerError> {
        let id = id.to_string();
        self.ask(|reply| Command::SetActiveLevel { id, active: level, reply })
            .unwrap_or_else(|| Err(Self::unreachable()))
    }

    fn activate_profile(&self, name: &str) -> Result<(), ManagerError> {
        let name = name.to_string();
        self.ask(|reply| Command::ActivateProfile { name, reply })
            .unwrap_or_else(|| Err(Self::unreachable()))
    }

    fn shutdown(&self) {
        let _ = self.tx.send(Command::Shutdown);
    }
}

/// Interpreta a saída de `systemctl --user is-enabled`. Qualquer coisa que não seja
/// "enabled"/"disabled" (serviço ausente, estático, mascarado…) conta como indisponível.
pub fn parse_is_enabled(stdout: &str) -> Autostart {
    match stdout.trim() {
        "enabled" | "enabled-runtime" => Autostart::Enabled,
        "disabled" => Autostart::Disabled,
        _ => Autostart::Unavailable,
    }
}

/// O systemd define `INVOCATION_ID` para os processos que ele inicia.
pub fn runs_under_systemd(invocation_id: Option<&str>) -> bool {
    invocation_id.is_some_and(|v| !v.is_empty())
}

pub trait ServiceControl: Send + Sync {
    fn autostart(&self) -> Autostart;
    fn set_autostart(&self, enable: bool) -> Result<(), String>;
    /// Para o serviço. `Ok(true)`: pediu ao systemd (o processo receberá SIGTERM);
    /// `Ok(false)`: o daemon não roda como serviço, quem chama deve encerrar por conta própria.
    fn stop(&self) -> Result<bool, String>;
}

pub struct SystemctlControl;

fn systemctl(args: &[&str]) -> std::io::Result<std::process::Output> {
    Process::new("systemctl").arg("--user").args(args).output()
}

impl ServiceControl for SystemctlControl {
    fn autostart(&self) -> Autostart {
        match systemctl(&["is-enabled", UNIT]) {
            Ok(out) => parse_is_enabled(&String::from_utf8_lossy(&out.stdout)),
            Err(_) => Autostart::Unavailable,
        }
    }

    fn set_autostart(&self, enable: bool) -> Result<(), String> {
        let verb = if enable { "enable" } else { "disable" };
        let out = systemctl(&[verb, UNIT]).map_err(|e| format!("não foi possível executar o systemctl: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    fn stop(&self) -> Result<bool, String> {
        let id = std::env::var("INVOCATION_ID").ok();
        if !runs_under_systemd(id.as_deref()) {
            return Ok(false);
        }
        let out = systemctl(&["stop", "--no-block", UNIT])
            .map_err(|e| format!("não foi possível executar o systemctl: {e}"))?;
        if out.status.success() {
            Ok(true)
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
}

pub trait Launcher: Send + Sync {
    fn open_gui(&self) -> Result<(), String>;
}

/// Abre a interface gráfica (`ghub-gui`, do Plano 4) se ela estiver instalada.
pub struct GuiLauncher;

impl Launcher for GuiLauncher {
    fn open_gui(&self) -> Result<(), String> {
        match Process::new("ghub-gui").spawn() {
            Ok(mut child) => {
                // Recolhe o processo filho para não deixar zumbi.
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err("a interface gráfica (ghub-gui) ainda não está instalada".into())
            }
            Err(e) => Err(format!("não foi possível abrir a interface: {e}")),
        }
    }
}
```

- [ ] **Step 5: Rodar tudo**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos. Se o compilador acusar importação não usada em `testing.rs` ou `link.rs`, remover só o `use` excedente.

- [ ] **Step 6: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): add manager, systemd and GUI launcher links with test fakes"
```

---

### Task 5: Executor de ações (`actions`)

**Files:**
- Create: `crates/ghubd/src/actions.rs`
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Consumes: `link::{ManagerLink, ServiceControl, Launcher}`, `notify::{Notification, Notifier}`, `tray_model::{pick_primary, Autostart}`.
- Produces (`ghubd::actions`):
  - `Action { SetDpiLevel(usize), ActivateProfile(String), ToggleAutostart, OpenGui, Quit }` (`Debug, Clone, PartialEq, Eq`).
  - `Outcome { Refresh, Exit }` (`Debug, Clone, Copy, PartialEq, Eq`).
  - `Executor::new(manager: Arc<dyn ManagerLink>, service: Arc<dyn ServiceControl>, launcher: Arc<dyn Launcher>, notifier: Arc<dyn Notifier>)` e `Executor::execute(&self, Action) -> Outcome`.

- [ ] **Step 1: Escrever os testes que falham**

`crates/ghubd/src/actions.rs` (só `use` e testes):

```rust
use std::sync::Arc;

use crate::link::{Launcher, ManagerLink, ServiceControl};
use crate::manager::ManagerError;
use crate::notify::{Notification, Notifier};
use crate::tray_model::{pick_primary, Autostart};

#[cfg(test)]
mod tests {
    use super::*;
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
```

Em `lib.rs` acrescentar `pub mod actions;`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p ghubd actions`
Expected: FAIL de compilação (`Executor`, `Action`, `Outcome` não definidos).

- [ ] **Step 3: Implementar**

Inserir em `actions.rs`, entre os `use` e os testes:

```rust
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
```

- [ ] **Step 4: Rodar tudo**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos.

- [ ] **Step 5: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): add tray action executor with error notifications"
```

---

### Task 6: Cola com o `ksni` (`tray`)

**Files:**
- Create: `crates/ghubd/src/tray.rs`
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Consumes: `actions::Action`, `tray_icon::{render, SIZES}`, `tray_model::{Entry, TrayView}`.
- Produces (`ghubd::tray`):
  - `GhubTray::new(view: TrayView, actions: Sender<Action>) -> GhubTray` e `GhubTray::set_view(&mut self, TrayView)`; `impl ksni::Tray for GhubTray` (id `ghub-linux`, categoria `Hardware`, estado `Active`, ícones por pixmap em `SIZES`, dica, `activate` abre a GUI, menu a partir de `view.entries`).
  - `type TrayHandle = ksni::blocking::Handle<GhubTray>`.
  - `spawn(view: TrayView, actions: Sender<Action>) -> Result<TrayHandle, ksni::Error>` (espera o `StatusNotifierWatcher` com `assume_sni_available(true)`).

- [ ] **Step 1: Escrever os testes que falham**

`crates/ghubd/src/tray.rs` (só `use` e testes):

```rust
use std::sync::mpsc::Sender;

use ksni::menu::{CheckmarkItem, RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{Category, Icon, MenuItem, Status, ToolTip};

use crate::actions::Action;
use crate::tray_icon::{render, IconState, Pixmap, SIZES};
use crate::tray_model::{Entry, TrayView};

#[cfg(test)]
mod tests {
    use super::*;
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
```

Em `lib.rs` acrescentar `pub mod tray;`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p ghubd tray::tests`
Expected: FAIL de compilação (`GhubTray` não definido).

- [ ] **Step 3: Implementar**

Inserir em `tray.rs`, entre os `use` e os testes:

```rust
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
```

- [ ] **Step 4: Rodar tudo**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos. Se `Status` não implementar `PartialEq`/`Debug` (o teste usa `assert_eq!`), troque a asserção por `matches!(t.status(), Status::Active)`; se `Category::Hardware` não existir nesta versão, use `Category::ApplicationStatus` e registre no relatório.

- [ ] **Step 5: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): add the ksni tray glue (icon, tooltip, menu and click actions)"
```

---

### Task 7: Hub (laços de eventos, atualização e ações) e `main`

**Files:**
- Create: `crates/ghubd/src/hub.rs`
- Modify: `crates/ghubd/src/lib.rs`
- Modify: `crates/ghubd/src/main.rs`

**Interfaces:**
- Consumes: Tasks 1 a 6, `manager::Event`.
- Produces (`ghubd::hub`):
  - `Wake` (struct unitária, `Debug, Clone, Copy`).
  - `collect_input(link: &dyn ManagerLink, service: &dyn ServiceControl) -> TrayInput`.
  - `run_fanout(events: Receiver<Event>, to_dbus: Sender<Event>, to_tray: Sender<Wake>, link: &dyn ManagerLink, notifier: &dyn Notifier)`: repassa cada evento ao D-Bus, avisa a bandeja e, em `BatteryLow`, envia **uma** notificação com o nome do dispositivo.
  - `run_refresher(wake: Receiver<Wake>, link: &dyn ManagerLink, service: &dyn ServiceControl, periodic: Duration, apply: &mut dyn FnMut(TrayView))`: aplica a vista inicial, coalesce rajadas (`COALESCE = 150 ms`) e atualiza também a cada `periodic`; retorna quando o canal fecha.
  - `run_actions(actions: Receiver<Action>, exec: &Executor, wake: Sender<Wake>)`: retorna ao receber `Outcome::Exit`.
- `main`: `ghubd --notify-test` envia uma notificação de teste e sai; `--help`/`-h` imprime o uso.

- [ ] **Step 1: Escrever os testes que falham**

`crates/ghubd/src/hub.rs` (só `use` e testes):

```rust
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use crate::actions::{Action, Executor, Outcome};
use crate::link::{ManagerLink, ServiceControl};
use crate::manager::Event;
use crate::notify::{Notification, Notifier};
use crate::tray_model::{build_view, pick_primary, TrayInput, TrayView};

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
```

Em `lib.rs` acrescentar `pub mod hub;`.

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p ghubd hub`
Expected: FAIL de compilação (`Wake`, `collect_input`, `run_fanout`… não definidos).

- [ ] **Step 3: Implementar o hub**

Inserir em `hub.rs`, entre os `use` e os testes:

```rust
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
```

Se o compilador reclamar de `unused import` de `Entry` no módulo de testes, remova só esse `use`, e o teste `unused_entry_import_is_exercised` que o usa **não** deve ser apagado: mantenha o `use` onde ele é necessário.

- [ ] **Step 4: Ligar tudo em `main.rs`**

Substituir `crates/ghubd/src/main.rs` por:

```rust
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
```

Nota: `wake_rx` e `action_rx` ficam sem uso quando a bandeja falha; isso só descarta os receptores (os remetentes passam a falhar em silêncio, o que é intencional).

- [ ] **Step 5: Compilar e testar**

Run: `RUSTFLAGS="-D warnings" cargo build --workspace && RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: compila sem avisos; todos os testes passam. Se a ordem das variáveis (`wake_tx` movido) causar erro de empréstimo, ajustar **o código** com clones, sem mudar o comportamento descrito, e listar a diferença no relatório. **Não execute o `ghubd`** nesta task.

- [ ] **Step 6: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): wire tray, notifications and actions into the daemon"
```

---

### Task 8: Ativação sob demanda e instalação

**Files:**
- Create: `packaging/dbus/org.ghublinux.Daemon.service`
- Modify: `packaging/install-user.sh`

**Interfaces:**
- Produces: arquivo de ativação D-Bus (instalado em `~/.local/share/dbus-1/services/`) que inicia `ghubd.service` quando um cliente chama o nome `org.ghublinux.Daemon`; `install-user.sh` com `--dry-run` (imprime o que faria, sem alterar nada e sem compilar).

- [ ] **Step 1: Arquivo de ativação**

`packaging/dbus/org.ghublinux.Daemon.service`:

```ini
[D-BUS Service]
Name=org.ghublinux.Daemon
Exec=@BINDIR@/ghubd
SystemdService=ghubd.service
```

- [ ] **Step 2: Script de instalação com `--dry-run`**

Substituir `packaging/install-user.sh` por:

```bash
#!/usr/bin/env bash
# Instala o ghubd para o usuário atual (sem sudo). Não habilita o serviço.
#   --dry-run   só imprime o que seria feito
set -euo pipefail

DRY=0
if [ "${1:-}" = "--dry-run" ]; then
    DRY=1
elif [ -n "${1:-}" ]; then
    echo "uso: $0 [--dry-run]" >&2
    exit 2
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
bindir="$HOME/.local/bin"
unitdir="$HOME/.config/systemd/user"
dbusdir="$HOME/.local/share/dbus-1/services"

run() {
    if [ "$DRY" = 1 ]; then
        echo "+ $*"
    else
        "$@"
    fi
}

run cargo build --release -p ghubd --manifest-path "$root/Cargo.toml"
run install -Dm755 "$root/target/release/ghubd" "$bindir/ghubd"
run install -Dm644 "$root/packaging/systemd/ghubd.service" "$unitdir/ghubd.service"

# O arquivo de ativação do D-Bus não expande variáveis: grava o caminho absoluto.
if [ "$DRY" = 1 ]; then
    echo "+ sed 's|@BINDIR@|$bindir|' $root/packaging/dbus/org.ghublinux.Daemon.service > $dbusdir/org.ghublinux.Daemon.service"
else
    mkdir -p "$dbusdir"
    sed "s|@BINDIR@|$bindir|" "$root/packaging/dbus/org.ghublinux.Daemon.service" \
        > "$dbusdir/org.ghublinux.Daemon.service"
fi

run systemctl --user daemon-reload

echo "Instalado. Para iniciar agora:        systemctl --user start ghubd"
echo "Para iniciar com a sessão (opcional): systemctl --user enable ghubd"
echo "(ou use \"Iniciar com o sistema\" no menu da bandeja)"
```

- [ ] **Step 3: Verificar**

Run:

```bash
chmod +x packaging/install-user.sh
bash -n packaging/install-user.sh && echo "sintaxe ok"
HOME=/tmp/ghub-dry-home bash packaging/install-user.sh --dry-run
```

Expected: `sintaxe ok`; a execução em `--dry-run` imprime 5 linhas `+ ...` (cargo build, três instalações incluindo o `sed`, e `systemctl --user daemon-reload`) mais as mensagens finais, com os caminhos sob `/tmp/ghub-dry-home`, e **não cria nada** (`ls /tmp/ghub-dry-home` falha com "No such file"). Conferir também `grep -c "SystemdService=ghubd.service" packaging/dbus/org.ghublinux.Daemon.service` → `1`.

- [ ] **Step 4: Commit**

```bash
git add packaging
git commit -m "feat(packaging): add D-Bus activation file and a dry-run installer"
```

---

### Task 9: Verificação no desktop do usuário e documentação

**Files:**
- Create: `docs/hardware/tray-verification.md`
- Modify: `docs/daemon.md`
- Modify: `docs/superpowers/specs/2026-10-12-ghub-linux-design.md` (seção 5.3)
- Modify: `README.md`

Esta task altera o ambiente do usuário e usa o mouse. **O controlador pede autorização antes dos Steps 4 e 6** (instalar em `~/.local`/`~/.config` e `systemctl --user enable`; escrever DPI). Nenhum passo usa `sudo`.

- [ ] **Step 1: Compilar e subir o daemon à mão (somente leitura)**

Com o mouse acordado: `cargo run -p ghubd` em segundo plano (log em arquivo). O **usuário** confere visualmente, na barra: (a) aparece o ícone do pinguim; (b) o tooltip mostra o nome, a bateria e o DPI; (c) o menu lista o dispositivo e a bateria, "DPI", "Perfil", "Iniciar com o sistema" (**desabilitado**, porque o serviço ainda não está instalado), "Abrir G HUB Linux" e "Sair e parar o serviço". Conferir também por D-Bus que o item registrou: `busctl --user get-property org.kde.StatusNotifierWatcher /StatusNotifierWatcher org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems`.
Expected: tudo isso, e `Devices` continua correto. Se algo faltar, registrar o log e parar.

- [ ] **Step 2: Estados do ícone**

O usuário deixa o mouse dormir/desliga: o ícone passa a "desconectado" (esmaecido), o tooltip diz "Inativo ou dormindo · <última bateria> %" e o submenu DPI fica desabilitado. Ao religar, volta ao normal. Registrar o que o usuário vê.

- [ ] **Step 3: Notificação**

Rodar `cargo run -q -p ghubd -- --notify-test`.
Expected: imprime "notificação de teste enviada" e o usuário vê a notificação no desktop. (A notificação real de bateria baixa não é provocada; ela é coberta por testes do `hub::run_fanout`.)

- [ ] **Step 4: Escrita pelo menu (pedir autorização)**

Com autorização: criar níveis com `busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 SetDpiLevels sauu <id> 3 800 1200 1600 1` (o id vem de `Devices`), abrir o menu e escolher outro nível em "DPI".
Expected: o DPI do mouse muda (conferir com `GetDeviceState`), a bolinha do rádio acompanha. Criar o perfil "Trabalho" (`CreateProfile s Trabalho`) e trocá-lo pelo menu "Perfil": o menu e `GetActiveProfile` refletem. No fim, restaurar `SetDpiLevels sauu <id> 1 1200 0` e `ActivateProfile s Padrão`, e `DeleteProfile s Trabalho`.

- [ ] **Step 5: Instalação e autostart (pedir autorização)**

Com autorização, e depois de encerrar o daemon de teste: `bash packaging/install-user.sh` (escreve só sob `$HOME`), depois `systemctl --user start ghubd`.
Expected: o ícone volta; "Iniciar com o sistema" agora fica **habilitado** e desmarcado. O usuário o marca pelo menu: `systemctl --user is-enabled ghubd` deve dizer `enabled`; desmarca: `disabled`. Testar a ativação sob demanda: `systemctl --user stop ghubd`, depois `busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 Devices` deve iniciar o serviço sozinho. Testar "Sair e parar o serviço": o processo some e `systemctl --user is-active ghubd` diz `inactive` (sem reiniciar sozinho).

- [ ] **Step 6: Decidir o estado final com o usuário**

Perguntar se quer deixar instalado e habilitado ("Iniciar com o sistema" marcado) ou desfazer (`systemctl --user disable --now ghubd` e remover os três arquivos instalados). Deixar exatamente como o usuário escolher e registrar.

- [ ] **Step 7: Documentar**

`docs/hardware/tray-verification.md`: data, commit, o que o usuário viu em cada passo (apenas o que foi observado), as escolhas finais do Step 6 e divergências. `docs/daemon.md`: acrescentar seções "Bandeja" (itens do menu, estados do ícone, clique esquerdo abre a GUI), "Notificações" (`--notify-test`, bateria baixa), "Iniciar com o sistema e ativação sob demanda" (unidade, arquivo de ativação D-Bus, `install-user.sh --dry-run`) e atualizar a lista de pendências (retirar o que este plano resolveu: bandeja, autostart, notificação, arquivo de ativação; manter "estado dormindo", "assíncrono", "Warnings", "sinais no #[interface]", "limiar configurável", "teste de hardware opt-in"). Spec 5.3: refletir a implementação (bandeja no daemon via `ksni`, esperando o watcher, coalescência, notificação por `org.freedesktop.Notifications`). `README.md`: uma linha sobre a bandeja apontando para `docs/daemon.md`.

- [ ] **Step 8: Verificação final e commit**

Encerrar qualquer `ghubd` de teste. Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS.

```bash
git add docs README.md
git commit -m "docs: document the tray, notifications and autostart with desktop verification"
```

---

## Auto-revisão (feita ao escrever)

- **Cobertura da spec (5.3):** bandeja no daemon via `ksni` (Tasks 1, 2, 6, 7); estados do ícone normal/bateria baixa/carregando/desconectado (1 e 2); menu com bateria/perfil, DPI, perfil, "Iniciar com o sistema", "Abrir" e "Sair e parar o serviço" (2 e 6); "Iniciar com o sistema" liga/desliga o `ghubd.service` do usuário (4 e 5); só o daemon inicia com a sessão (a GUI abre sob demanda, `GuiLauncher`); notificação de bateria baixa (3 e 7); ativação sob demanda por D-Bus (8). A GUI em si e o limiar configurável ficam fora (Plano 4 / pendências).
- **Achados anteriores incorporados:** o daemon não morre sem bandeja (`assume_sni_available`, falha de `spawn` tratada); mouse dormindo mostra a última bateria (modelo) sem piscar; coalescência de rajadas; erros viram notificação; autostart indisponível desabilita o item.
- **Decisão provisória do usuário respeitada:** nenhum código novo altera a reaplicação do perfil ao acordar; consta nas restrições globais.
- **Consistência de tipos:** `Action`, `Outcome`, `Executor`, `ManagerLink`, `ServiceControl`, `Launcher`, `Notifier`, `Notification`, `Urgency`, `TrayView`, `Entry`, `TrayInput`, `Autostart`, `IconState`, `Pixmap`, `Wake` têm as mesmas assinaturas em todas as tasks; os dublês de `testing.rs` (Task 4) são os usados nas Tasks 5 e 7.
- **Sem placeholders:** todo passo de código traz o código. A parte mais delicada é o `main.rs` da Task 7 (propriedade dos canais entre as threads), que só é exercitado de verdade na Task 9.
- **Riscos conhecidos:** o código novo não foi compilado; o executor deve corrigir o **código** (não os testes) quando algo divergir do que os nomes dos testes descrevem e reportar `DONE_WITH_CONCERNS`. `DbusNotifier` e `ksni::spawn` só são exercitados na Task 9. O nome `Category::Hardware` e a derivação `PartialEq` de `Status` são suposições a confirmar na Task 6.
