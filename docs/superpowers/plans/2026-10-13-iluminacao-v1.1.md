# Iluminação v1.1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Escolher efeito e cor do LED do G703 LS pela aba Iluminação e gravá-los no perfil onboard ativo do mouse.

**Architecture:** Um módulo `hidpp::features::onboard` lê/grava o setor do perfil ativo (bytes opacos + CRC) e só interpreta as 4 entradas de LED. O `ghubd` expõe `LedEffects/GetLed/SetLed` no D-Bus via a thread gerente; a GUI ganha a página `Lighting.qml` ligada ao controller por `Command::SetLed`.

**Tech Stack:** Rust (workspace edition 2021), HID++ 2.0 sobre hidraw, zbus 5, cxx-qt + QML (Qt 6), clap 4.

**Spec:** `docs/superpowers/specs/2026-10-13-iluminacao-v1.1-design.md` (fatos de hardware em `docs/onboard-profiles.md`, backup em `docs/onboard-g703ls-backup.txt`).

## Global Constraints

- Gravar só no **perfil ativo** (`0x8100 fn4`) e só nos bytes das entradas de LED (offsets `0xd0`, `0xdb`, `0xe6`, `0xf1`, 11 bytes cada) mais o CRC (bytes 253–254, CRC-16/CCITT-FALSE poli `0x1021` init `0xFFFF` big-endian dos 253 primeiros).
- Setor tem 255 bytes. Leitura: `fn5 [setor(2), offset(2)]` (16 bytes; offsets 0,16,…,224 e 239). Escrita: `fn6 [setor(2), 0,0, 0,255]`, `fn7` com 16 bytes por chamada (último bloco completado com `ff`), `fn8`. Recarregar com `fn3 [0, número do perfil]`.
- Nunca gravar sem: CRC válido na leitura, cópia de segurança do setor, leitura de volta igual (senão regravar a cópia e devolver erro).
- O host não guarda estado de LED (`profiles.toml` intocado). Sem toggle "apagar ao suspender". Sem sincronização contínua com cores do sistema.
- Nomes de efeito no D-Bus/CLI: `off`, `fixed`, `cycle`, `breathing`. Cor: `#RRGGBB`.
- Mensagens ao usuário em português; código e identificadores em inglês, como no resto do repositório.
- Cada gravação no mouse real exige aprovação explícita do usuário (flash). Testes automatizados nunca tocam o hardware.
- Commits terminam com `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.
- Verificação padrão antes de cada commit: `RUSTFLAGS="-D warnings" cargo test --workspace --exclude ghub-gui` (GUI: `cargo test -p ghub-gui --features fake-daemon`).

## Desvios da spec (decididos ao ler o código)

1. `DeviceState` **não** ganha `led: Option<LedInfo>`: mudar a assinatura D-Bus de `DeviceState` quebraria o contrato estável e o roundtrip em `ghub-ipc`. Em vez disso há um método `LedEffects(id) -> as` (lista vazia = sem suporte). A Task 11 atualiza a spec.
2. A conversão efeito/cor ↔ `Led` (strings) mora em `hidpp::features::onboard::Led::{from_parts, parts}`, compartilhada por `ghubctl` e `ghubd`.
3. `SetLed` usa timeout de 15 s no serviço D-Bus (a gravação é lenta); as demais chamadas seguem com 5 s.
4. A página Lighting entra como **índice 3** do `StackLayout` (depois de Configurações, índice 2), para não renumerar páginas existentes; a aba aparece na 3ª posição da barra.

## Review Focus

1. Mouse com CRC inválido no setor ativo → `SetLed` recusa e não escreve nada (Task 2, Task 7).
2. Leitura de volta diferente do gravado → o backup é regravado e o erro chega à GUI (Task 2).
3. Falha ao salvar a cópia de segurança (disco cheio/permissão) → aborta antes de escrever (Task 2).
4. Cor digitada inválida (`#GGG`, curta, sem `#`) e efeito desconhecido → `InvalidArgs`, nada gravado (Task 1, Task 7).
5. Mouse dormindo/offline ao clicar Aplicar → erro legível, UI sai do estado "Gravando…" (Task 8, Task 10).

## File Structure

- Create `crates/hidpp/src/features/onboard.rs` — CRC, `Led`/`LedKind`, `ActiveProfile`, leitura/gravação, `set_led`, backup. Um arquivo coeso (~400 linhas com testes).
- Create `crates/hidpp/src/features/onboard_fake.rs` — `MemOnboard`, transporte falso com memória onboard (pública, como `FakeTransport`).
- Modify `crates/hidpp/src/features/mod.rs`, `crates/hidpp/src/error.rs`.
- Modify `crates/ghubctl/src/main.rs` — subcomando `led`.
- Modify `crates/ghub-ipc/src/lib.rs` — `LedSetting` + métodos do proxy.
- Modify `crates/ghubd/src/{mouse.rs,testing.rs,manager.rs,service.rs}` e `docs/daemon.md`.
- Modify `crates/ghub-gui/src/client/{mod.rs,real.rs,fake.rs,controller.rs}`, `src/model/{state.rs,props.rs,led.rs(novo),mod.rs}`, `src/backend.rs`.
- Create `crates/ghub-gui/qml/pages/Lighting.qml`, `crates/ghub-gui/qml/components/LedPreview.qml`; modify `qml/AppWindow.qml`, `qml/Smoke.qml`, `build.rs`, `tests/smoke.rs`.
- Modify `docs/gui.md`, `docs/gui-verification.md`, a spec.

---

### Task 1: Codec do LED e CRC (`hidpp::features::onboard`, parte pura)

**Files:**
- Create: `crates/hidpp/src/features/onboard.rs`
- Modify: `crates/hidpp/src/features/mod.rs` (adicionar `pub mod onboard;`)
- Modify: `crates/hidpp/src/error.rs` (variantes novas)

**Interfaces:**
- Produces (usados pelas Tasks 2–11):
  - `pub const ONBOARD_PROFILES: u16 = 0x8100; pub const SECTOR_SIZE: usize = 255; pub type Sector = [u8; SECTOR_SIZE];`
  - `pub fn crc16(data: &[u8]) -> u16; pub fn verify(s: &Sector) -> bool; pub fn seal(s: &mut Sector);`
  - `pub enum LedKind { Off, Fixed, Cycle, Breathing }` com `pub const ALL: [LedKind; 4]` e `pub fn name(self) -> &'static str`.
  - `pub enum Led { Off, Fixed { rgb: [u8; 3] }, Cycle { period_ms: u16, intensity: u8 }, Breathing { rgb: [u8; 3], period_ms: u16, waveform: u8, intensity: u8 } }` com `kind()`, `parse(&[u8; 11]) -> Result<Led>`, `encode() -> [u8; 11]`, `from_parts(effect: &str, color: &str, period_ms: u32, intensity: u32) -> Result<Led, String>`, `parts(&self) -> (&'static str, String, u16, u8)`.
  - `HidppError::BadChecksum`, `HidppError::VerifyFailed`.

- [ ] **Step 1: Variantes de erro**

Em `crates/hidpp/src/error.rs`, antes de `Io`:

```rust
    #[error("a memória onboard está corrompida (CRC inválido)")]
    BadChecksum,
    #[error("a leitura de volta da memória onboard difere do que foi gravado")]
    VerifyFailed,
```

Teste no módulo `tests` do mesmo arquivo:

```rust
    #[test]
    fn onboard_errors_are_readable() {
        assert!(HidppError::BadChecksum.to_string().contains("CRC"));
        assert!(HidppError::VerifyFailed.to_string().contains("leitura de volta"));
    }
```

- [ ] **Step 2: Escrever os testes que falham**

Criar `crates/hidpp/src/features/onboard.rs` só com o módulo de testes (o resto vem no Step 4) e registrar `pub mod onboard;` em `features/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_matches_the_ccitt_false_check_value() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    #[test]
    fn seal_then_verify_roundtrips_and_detects_corruption() {
        let mut s: Sector = [0xff; SECTOR_SIZE];
        s[3] = 0x42;
        assert!(!verify(&s));
        seal(&mut s);
        assert!(verify(&s));
        s[10] ^= 1;
        assert!(!verify(&s));
    }

    #[test]
    fn parses_the_cycle_entry_read_from_the_g703() {
        // Bytes reais do setor 1 do G703 LS (docs/onboard-profiles.md).
        let raw = [0x03, 0, 0, 0, 0, 0, 0x1f, 0x40, 0, 0, 0];
        assert_eq!(Led::parse(&raw), Ok(Led::Cycle { period_ms: 8000, intensity: 0 }));
        assert_eq!(Led::Cycle { period_ms: 8000, intensity: 0 }.encode(), raw);
    }

    #[test]
    fn fixed_and_off_layouts() {
        let fixed = Led::Fixed { rgb: [0xff, 0x00, 0x00] };
        assert_eq!(fixed.encode(), [0x01, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(Led::parse(&fixed.encode()), Ok(fixed));
        assert_eq!(Led::Off.encode(), [0u8; 11]);
        assert_eq!(Led::parse(&[0u8; 11]), Ok(Led::Off));
    }

    #[test]
    fn breathing_roundtrips() {
        let b = Led::Breathing { rgb: [1, 2, 3], period_ms: 4000, waveform: 1, intensity: 50 };
        assert_eq!(b.encode(), [0x0a, 1, 2, 3, 0x0f, 0xa0, 1, 50, 0, 0, 0]);
        assert_eq!(Led::parse(&b.encode()), Ok(b));
    }

    #[test]
    fn unknown_mode_is_an_error() {
        assert_eq!(
            Led::parse(&[0x7f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            Err(HidppError::UnexpectedResponse("unknown LED mode"))
        );
    }

    #[test]
    fn from_parts_accepts_valid_input_and_normalizes_color() {
        assert_eq!(
            Led::from_parts("fixed", "#fe8019", 0, 0),
            Ok(Led::Fixed { rgb: [0xfe, 0x80, 0x19] })
        );
        assert_eq!(Led::from_parts("off", "", 0, 0), Ok(Led::Off));
        assert_eq!(
            Led::from_parts("cycle", "", 8000, 0),
            Ok(Led::Cycle { period_ms: 8000, intensity: 0 })
        );
        assert_eq!(
            Led::from_parts("breathing", "#00ff00", 4000, 100),
            Ok(Led::Breathing { rgb: [0, 255, 0], period_ms: 4000, waveform: BREATHING_WAVEFORM, intensity: 100 })
        );
    }

    #[test]
    fn from_parts_rejects_bad_input() {
        for (e, c, p, i) in [
            ("sparkle", "", 0, 0),          // efeito desconhecido
            ("fixed", "", 0, 0),            // cor obrigatória
            ("fixed", "#GGGGGG", 0, 0),     // não hexadecimal
            ("fixed", "#fff", 0, 0),        // curta
            ("fixed", "fe8019", 0, 0),      // sem '#'
            ("cycle", "", 0, 0),            // período zero
            ("cycle", "", 70000, 0),        // período não cabe em u16
            ("breathing", "#ff0000", 4000, 101), // intensidade > 100
        ] {
            assert!(Led::from_parts(e, c, p, i).is_err(), "{e} {c} {p} {i}");
        }
    }

    #[test]
    fn parts_is_the_inverse_of_from_parts() {
        for led in [
            Led::Off,
            Led::Fixed { rgb: [0xfe, 0x80, 0x19] },
            Led::Cycle { period_ms: 8000, intensity: 0 },
            Led::Breathing { rgb: [0, 255, 0], period_ms: 4000, waveform: BREATHING_WAVEFORM, intensity: 100 },
        ] {
            let (e, c, p, i) = led.parts();
            assert_eq!(Led::from_parts(e, &c, u32::from(p), u32::from(i)), Ok(led));
        }
        assert_eq!(Led::Fixed { rgb: [0xfe, 0x80, 0x19] }.parts().1, "#FE8019");
    }
}
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p hidpp onboard`
Expected: FAIL de compilação (`crc16`, `Led`… não definidos).

- [ ] **Step 4: Implementação mínima**

No topo de `crates/hidpp/src/features/onboard.rs` (acima do `mod tests`):

```rust
//! Perfis onboard (`0x8100`): leitura/gravação do setor do perfil ativo e o LED que ele guarda.
//! Formato conferido no G703 LS (docs/onboard-profiles.md).

use crate::error::{HidppError, Result};

pub const ONBOARD_PROFILES: u16 = 0x8100;
pub const SECTOR_SIZE: usize = 255;
pub type Sector = [u8; SECTOR_SIZE];

const CRC_AT: usize = SECTOR_SIZE - 2;
/// Quatro entradas de LED de 11 bytes no fim do perfil; hoje o mouse as mantém iguais.
pub const LED_OFFSETS: [usize; 4] = [0xd0, 0xdb, 0xe6, 0xf1];
pub const LED_LEN: usize = 11;
/// Forma de onda da respiração (layout do libratbag; validar no hardware, Task 4).
pub const BREATHING_WAVEFORM: u8 = 0x00;

const MODE_OFF: u8 = 0x00;
const MODE_FIXED: u8 = 0x01;
const MODE_CYCLE: u8 = 0x03;
const MODE_BREATHING: u8 = 0x0a;

/// CRC-16/CCITT-FALSE (poli 0x1021, init 0xFFFF).
pub fn crc16(data: &[u8]) -> u16 {
    let mut c: u16 = 0xffff;
    for &x in data {
        c ^= u16::from(x) << 8;
        for _ in 0..8 {
            c = if c & 0x8000 != 0 { (c << 1) ^ 0x1021 } else { c << 1 };
        }
    }
    c
}

pub fn verify(s: &Sector) -> bool {
    crc16(&s[..CRC_AT]).to_be_bytes() == [s[CRC_AT], s[CRC_AT + 1]]
}

pub fn seal(s: &mut Sector) {
    let c = crc16(&s[..CRC_AT]).to_be_bytes();
    s[CRC_AT] = c[0];
    s[CRC_AT + 1] = c[1];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedKind {
    Off,
    Fixed,
    Cycle,
    Breathing,
}

impl LedKind {
    pub const ALL: [LedKind; 4] = [LedKind::Off, LedKind::Fixed, LedKind::Cycle, LedKind::Breathing];

    pub fn name(self) -> &'static str {
        match self {
            LedKind::Off => "off",
            LedKind::Fixed => "fixed",
            LedKind::Cycle => "cycle",
            LedKind::Breathing => "breathing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Led {
    Off,
    Fixed { rgb: [u8; 3] },
    /// `intensity` 0 = 100 %.
    Cycle { period_ms: u16, intensity: u8 },
    Breathing { rgb: [u8; 3], period_ms: u16, waveform: u8, intensity: u8 },
}

fn color_text(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn parse_color(text: &str) -> std::result::Result<[u8; 3], String> {
    let bad = || format!("cor inválida: use o formato #RRGGBB (recebido \"{text}\")");
    let hex = text.strip_prefix('#').ok_or_else(bad)?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let v = u32::from_str_radix(hex, 16).map_err(|_| bad())?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl Led {
    pub fn kind(&self) -> LedKind {
        match self {
            Led::Off => LedKind::Off,
            Led::Fixed { .. } => LedKind::Fixed,
            Led::Cycle { .. } => LedKind::Cycle,
            Led::Breathing { .. } => LedKind::Breathing,
        }
    }

    pub fn parse(raw: &[u8; LED_LEN]) -> Result<Led> {
        Ok(match raw[0] {
            MODE_OFF => Led::Off,
            MODE_FIXED => Led::Fixed { rgb: [raw[1], raw[2], raw[3]] },
            MODE_CYCLE => Led::Cycle { period_ms: u16::from_be_bytes([raw[6], raw[7]]), intensity: raw[8] },
            MODE_BREATHING => Led::Breathing {
                rgb: [raw[1], raw[2], raw[3]],
                period_ms: u16::from_be_bytes([raw[4], raw[5]]),
                waveform: raw[6],
                intensity: raw[7],
            },
            _ => return Err(HidppError::UnexpectedResponse("unknown LED mode")),
        })
    }

    pub fn encode(&self) -> [u8; LED_LEN] {
        let mut b = [0u8; LED_LEN];
        match *self {
            Led::Off => {}
            Led::Fixed { rgb } => {
                b[0] = MODE_FIXED;
                b[1..4].copy_from_slice(&rgb);
            }
            Led::Cycle { period_ms, intensity } => {
                b[0] = MODE_CYCLE;
                b[6..8].copy_from_slice(&period_ms.to_be_bytes());
                b[8] = intensity;
            }
            Led::Breathing { rgb, period_ms, waveform, intensity } => {
                b[0] = MODE_BREATHING;
                b[1..4].copy_from_slice(&rgb);
                b[4..6].copy_from_slice(&period_ms.to_be_bytes());
                b[6] = waveform;
                b[7] = intensity;
            }
        }
        b
    }

    /// Monta um `Led` a partir do que chega por D-Bus/CLI. `color` só vale em fixed/breathing e
    /// `period_ms` só em cycle/breathing; o resto é ignorado.
    pub fn from_parts(
        effect: &str,
        color: &str,
        period_ms: u32,
        intensity: u32,
    ) -> std::result::Result<Led, String> {
        let period = || match u16::try_from(period_ms) {
            Ok(p) if p > 0 => Ok(p),
            _ => Err(format!("período inválido: {period_ms} ms (use de 1 a 65535)")),
        };
        let level = || match u8::try_from(intensity) {
            Ok(i) if i <= 100 => Ok(i),
            _ => Err(format!("intensidade inválida: {intensity} (use de 0 a 100)")),
        };
        match effect {
            "off" => Ok(Led::Off),
            "fixed" => Ok(Led::Fixed { rgb: parse_color(color)? }),
            "cycle" => Ok(Led::Cycle { period_ms: period()?, intensity: level()? }),
            "breathing" => Ok(Led::Breathing {
                rgb: parse_color(color)?,
                period_ms: period()?,
                waveform: BREATHING_WAVEFORM,
                intensity: level()?,
            }),
            other => Err(format!("efeito desconhecido: \"{other}\"")),
        }
    }

    /// `(efeito, cor "#RRGGBB" ou "", período, intensidade)`; inverso de `from_parts`.
    pub fn parts(&self) -> (&'static str, String, u16, u8) {
        match *self {
            Led::Off => ("off", String::new(), 0, 0),
            Led::Fixed { rgb } => ("fixed", color_text(rgb), 0, 0),
            Led::Cycle { period_ms, intensity } => ("cycle", String::new(), period_ms, intensity),
            Led::Breathing { rgb, period_ms, intensity, .. } => ("breathing", color_text(rgb), period_ms, intensity),
        }
    }
}
```

- [ ] **Step 5: Rodar e ver passar**

Run: `RUSTFLAGS="-D warnings" cargo test -p hidpp`
Expected: PASS (todos os testes do crate, incluindo os novos).

- [ ] **Step 6: Commit**

```bash
git add crates/hidpp
git commit -m "feat(hidpp): onboard LED codec and CRC-16 for profile sectors

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Leitura/gravação do setor, `set_led` com salvaguardas, backup e `MemOnboard`

**Files:**
- Create: `crates/hidpp/src/features/onboard_fake.rs`
- Modify: `crates/hidpp/src/features/onboard.rs`, `crates/hidpp/src/features/mod.rs` (`pub mod onboard_fake;`)

**Interfaces:**
- Consumes: Task 1 (`Sector`, `verify`, `seal`, `Led`, `LED_OFFSETS`, `LED_LEN`, `ONBOARD_PROFILES`, `HidppError::{BadChecksum, VerifyFailed}`), `Device::call`.
- Produces:
  - `pub struct ActiveProfile { pub number: u8, pub sector: u16, pub bytes: Sector }` com `led(&self) -> Result<Led>` e `with_led(&self, &Led) -> ActiveProfile`.
  - `pub fn read_sector<T: Transport>(dev: &mut Device<T>, sector: u16) -> Result<Sector>`
  - `pub fn write_sector<T: Transport>(dev: &mut Device<T>, sector: u16, bytes: &Sector) -> Result<()>`
  - `pub fn read_active_profile<T: Transport>(dev: &mut Device<T>) -> Result<ActiveProfile>`
  - `pub fn read_led<T: Transport>(dev: &mut Device<T>) -> Result<Led>`
  - `pub fn set_led<T: Transport>(dev: &mut Device<T>, led: &Led, backup: impl FnOnce(&ActiveProfile) -> std::io::Result<()>) -> Result<()>`
  - `pub fn default_backup_dir() -> PathBuf`, `pub fn save_backup(dir: &Path, tag: &str, p: &ActiveProfile) -> std::io::Result<PathBuf>`
  - `onboard_fake::MemOnboard` (`new(feature_index: u8, led: &Led)`, campos públicos `sectors: HashMap<u16, Sector>`, `reloads: Vec<u8>`, `commits: Vec<u16>`, `corrupt_next_commit: bool`; implementa `Transport`) e `onboard_fake::sample_profile_sector(led: &Led) -> Sector`.

- [ ] **Step 1: `MemOnboard` (dublê com memória)**

Criar `crates/hidpp/src/features/onboard_fake.rs`:

```rust
//! Transporte falso com uma memória onboard simulada (diretório + um perfil), para testes.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use super::onboard::{seal, Led, Sector, LED_LEN, LED_OFFSETS, SECTOR_SIZE};
use crate::error::Result;
use crate::transport::{long_reply, Transport};

/// Setor de perfil válido (CRC selado) com `led` nas quatro entradas.
pub fn sample_profile_sector(led: &Led) -> Sector {
    let mut s: Sector = [0xff; SECTOR_SIZE];
    s[0] = 0x01; // taxa
    for o in LED_OFFSETS {
        s[o..o + LED_LEN].copy_from_slice(&led.encode());
    }
    seal(&mut s);
    s
}

pub struct MemOnboard {
    feature_index: u8,
    pub sectors: HashMap<u16, Sector>,
    profile_number: u8,
    pub reloads: Vec<u8>,
    /// Setores efetivamente gravados (um por `fn8`).
    pub commits: Vec<u16>,
    /// A próxima gravação sai com um byte trocado (simula falha de flash).
    pub corrupt_next_commit: bool,
    cursor: u16,
    staging: Vec<u8>,
    incoming: VecDeque<Vec<u8>>,
}

impl MemOnboard {
    /// Perfil 1 no setor 1, diretório no setor 0, com `led` nas quatro entradas.
    pub fn new(feature_index: u8, led: &Led) -> Self {
        let mut dir: Sector = [0xff; SECTOR_SIZE];
        dir[..4].copy_from_slice(&[0x00, 0x01, 0x01, 0x00]);
        seal(&mut dir);
        MemOnboard {
            feature_index,
            sectors: HashMap::from([(0, dir), (1, sample_profile_sector(led))]),
            profile_number: 1,
            reloads: vec![],
            commits: vec![],
            corrupt_next_commit: false,
            cursor: 0,
            staging: vec![],
            incoming: VecDeque::new(),
        }
    }
}

impl Transport for MemOnboard {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        assert_eq!(data[2], self.feature_index, "feature errada");
        let (func, p) = (data[3] >> 4, &data[4..]);
        let reply: Vec<u8> = match func {
            3 => {
                self.reloads.push(p[1]);
                vec![]
            }
            4 => vec![0, self.profile_number],
            5 => {
                let sector = u16::from_be_bytes([p[0], p[1]]);
                let off = usize::from(u16::from_be_bytes([p[2], p[3]]));
                let mem = &self.sectors[&sector];
                (0..16).map(|i| *mem.get(off + i).unwrap_or(&0xff)).collect()
            }
            6 => {
                self.cursor = u16::from_be_bytes([p[0], p[1]]);
                self.staging.clear();
                vec![]
            }
            7 => {
                self.staging.extend_from_slice(&p[..16]);
                vec![]
            }
            8 => {
                let mut s: Sector = [0; SECTOR_SIZE];
                s.copy_from_slice(&self.staging[..SECTOR_SIZE]);
                if std::mem::take(&mut self.corrupt_next_commit) {
                    s[10] ^= 0xff;
                }
                self.sectors.insert(self.cursor, s);
                self.commits.push(self.cursor);
                vec![]
            }
            other => panic!("função {other} não simulada"),
        };
        self.incoming.push_back(long_reply(data[1], data[2], data[3], &reply));
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>> {
        Ok(self.incoming.pop_front())
    }
}
```

Registrar em `features/mod.rs`: `pub mod onboard_fake;`.

- [ ] **Step 2: Escrever os testes que falham**

Acrescentar ao `mod tests` de `onboard.rs`:

```rust
    use super::super::onboard_fake::{sample_profile_sector, MemOnboard};
    use crate::device::Device;
    use std::collections::HashMap;

    const FI: u8 = 8;
    const CYCLE: Led = Led::Cycle { period_ms: 8000, intensity: 0 };
    const RED: Led = Led::Fixed { rgb: [0xff, 0, 0] };

    fn dev(mem: MemOnboard) -> Device<MemOnboard> {
        Device::with_features(mem, 1, HashMap::from([(ONBOARD_PROFILES, FI)]))
    }

    #[test]
    fn reads_the_active_profile_through_the_directory() {
        let mut d = dev(MemOnboard::new(FI, &CYCLE));
        let p = read_active_profile(&mut d).unwrap();
        assert_eq!((p.number, p.sector), (1, 1));
        assert_eq!(p.led(), Ok(CYCLE));
        assert_eq!(read_led(&mut d), Ok(CYCLE));
    }

    #[test]
    fn with_led_changes_only_the_led_entries_and_the_crc() {
        let base = ActiveProfile { number: 1, sector: 1, bytes: sample_profile_sector(&CYCLE) };
        let new = base.with_led(&RED);
        assert!(verify(&new.bytes));
        for i in 0..SECTOR_SIZE - 2 {
            let in_led = LED_OFFSETS.iter().any(|o| (*o..*o + LED_LEN).contains(&i));
            if !in_led {
                assert_eq!(new.bytes[i], base.bytes[i], "byte {i:#x} mudou");
            }
        }
        assert_eq!(new.led(), Ok(RED));
    }

    #[test]
    fn set_led_backs_up_writes_verifies_and_reloads() {
        let mut d = dev(MemOnboard::new(FI, &CYCLE));
        let mut saved = vec![];
        set_led(&mut d, &RED, |p| {
            saved.push(p.clone());
            Ok(())
        })
        .unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].led(), Ok(CYCLE));
        let mem = d.into_transport();
        assert_eq!(mem.commits, vec![1]);
        assert_eq!(mem.reloads, vec![1]);
        assert!(verify(&mem.sectors[&1]));
        assert_eq!(ActiveProfile { number: 1, sector: 1, bytes: mem.sectors[&1] }.led(), Ok(RED));
    }

    #[test]
    fn set_led_with_the_same_led_neither_backs_up_nor_writes() {
        let mut d = dev(MemOnboard::new(FI, &CYCLE));
        set_led(&mut d, &CYCLE, |_| panic!("não devia salvar backup")).unwrap();
        let mem = d.into_transport();
        assert!(mem.commits.is_empty() && mem.reloads.is_empty());
    }

    #[test]
    fn refuses_a_sector_with_a_bad_crc_and_writes_nothing() {
        let mut mem = MemOnboard::new(FI, &CYCLE);
        mem.sectors.get_mut(&1).unwrap()[20] ^= 0xff;
        let mut d = dev(mem);
        assert_eq!(set_led(&mut d, &RED, |_| Ok(())), Err(HidppError::BadChecksum));
        assert!(d.into_transport().commits.is_empty());
    }

    #[test]
    fn a_failed_backup_aborts_before_writing() {
        let mut d = dev(MemOnboard::new(FI, &CYCLE));
        let r = set_led(&mut d, &RED, |_| Err(std::io::Error::other("disco cheio")));
        assert!(matches!(r, Err(HidppError::Io(m)) if m.contains("disco cheio")));
        assert!(d.into_transport().commits.is_empty());
    }

    #[test]
    fn a_verify_mismatch_restores_the_original_sector() {
        let mut mem = MemOnboard::new(FI, &CYCLE);
        mem.corrupt_next_commit = true;
        let original = mem.sectors[&1];
        let mut d = dev(mem);
        assert_eq!(set_led(&mut d, &RED, |_| Ok(())), Err(HidppError::VerifyFailed));
        let mem = d.into_transport();
        assert_eq!(mem.commits, vec![1, 1], "gravou e regravou o original");
        assert_eq!(mem.sectors[&1], original);
        assert!(mem.reloads.is_empty());
    }

    #[test]
    fn unsupported_feature_is_reported() {
        let mut d = Device::with_features(MemOnboard::new(FI, &CYCLE), 1, HashMap::new());
        assert_eq!(read_led(&mut d), Err(HidppError::FeatureNotSupported(ONBOARD_PROFILES)));
    }

    #[test]
    fn save_backup_writes_the_raw_sector() {
        let dir = std::env::temp_dir().join(format!("ghub-onboard-bk-{}", std::process::id()));
        let p = ActiveProfile { number: 1, sector: 1, bytes: sample_profile_sector(&CYCLE) };
        let path = save_backup(&dir, "dev1", &p).unwrap();
        assert!(path.file_name().unwrap().to_string_lossy().starts_with("dev1-sector0001-"));
        assert_eq!(std::fs::read(&path).unwrap(), p.bytes.to_vec());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn default_backup_dir_follows_xdg_data_home() {
        // Só confere o sufixo: o prefixo depende do ambiente.
        assert!(default_backup_dir().ends_with("ghub-linux/onboard-backup"));
    }
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p hidpp onboard`
Expected: FAIL de compilação (`ActiveProfile`, `read_active_profile`, `set_led`… não existem).

- [ ] **Step 4: Implementação**

Em `onboard.rs`, acrescentar acima do `mod tests` (e `use crate::device::Device; use crate::transport::Transport; use std::path::{Path, PathBuf};` no topo):

```rust
const FN_RELOAD: u8 = 3;
const FN_CURRENT: u8 = 4;
const FN_READ: u8 = 5;
const FN_WRITE_START: u8 = 6;
const FN_WRITE: u8 = 7;
const FN_WRITE_END: u8 = 8;

pub fn read_sector<T: Transport>(dev: &mut Device<T>, sector: u16) -> Result<Sector> {
    let mut out: Sector = [0; SECTOR_SIZE];
    let [sh, sl] = sector.to_be_bytes();
    // 0,16,…,224 cobrem 0..240; a última leitura em 239 cobre o resto (sobrepõe 1 byte).
    for off in (0u16..240).step_by(16).chain([SECTOR_SIZE as u16 - 16]) {
        let [oh, ol] = off.to_be_bytes();
        let r = dev.call(ONBOARD_PROFILES, FN_READ, &[sh, sl, oh, ol])?;
        let at = usize::from(off);
        let n = 16.min(SECTOR_SIZE - at);
        out[at..at + n].copy_from_slice(&r[..n]);
    }
    Ok(out)
}

pub fn write_sector<T: Transport>(dev: &mut Device<T>, sector: u16, bytes: &Sector) -> Result<()> {
    let [sh, sl] = sector.to_be_bytes();
    dev.call(ONBOARD_PROFILES, FN_WRITE_START, &[sh, sl, 0, 0, 0, SECTOR_SIZE as u8])?;
    for chunk in bytes.chunks(16) {
        let mut block = [0xffu8; 16];
        block[..chunk.len()].copy_from_slice(chunk);
        dev.call(ONBOARD_PROFILES, FN_WRITE, &block)?;
    }
    dev.call(ONBOARD_PROFILES, FN_WRITE_END, &[])?;
    Ok(())
}

/// Perfil onboard em uso: número (1 a N), setor onde mora e seu conteúdo (CRC já conferido).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProfile {
    pub number: u8,
    pub sector: u16,
    pub bytes: Sector,
}

impl ActiveProfile {
    fn entry(&self, i: usize) -> [u8; LED_LEN] {
        let o = LED_OFFSETS[i];
        self.bytes[o..o + LED_LEN].try_into().expect("11 bytes")
    }

    /// O LED da primeira entrada (as quatro são mantidas iguais).
    pub fn led(&self) -> Result<Led> {
        Led::parse(&self.entry(0))
    }

    /// Cópia com `led` nas quatro entradas e o CRC refeito; o resto dos bytes não muda.
    pub fn with_led(&self, led: &Led) -> ActiveProfile {
        let mut n = self.clone();
        for o in LED_OFFSETS {
            n.bytes[o..o + LED_LEN].copy_from_slice(&led.encode());
        }
        seal(&mut n.bytes);
        n
    }
}

/// Lê o perfil ativo: `fn4` dá o número (1-based; confirmado só para o perfil 1) e o diretório
/// (setor 0, entradas de 4 bytes `[setor(2), habilitado, 0]`) diz onde ele mora.
pub fn read_active_profile<T: Transport>(dev: &mut Device<T>) -> Result<ActiveProfile> {
    let number = dev.call(ONBOARD_PROFILES, FN_CURRENT, &[])?[1];
    if number == 0 {
        return Err(HidppError::UnexpectedResponse("no active onboard profile"));
    }
    let dir = read_sector(dev, 0)?;
    if !verify(&dir) {
        return Err(HidppError::BadChecksum);
    }
    let at = (usize::from(number) - 1) * 4;
    if at + 2 > CRC_AT {
        return Err(HidppError::UnexpectedResponse("onboard profile outside the directory"));
    }
    let sector = u16::from_be_bytes([dir[at], dir[at + 1]]);
    if sector == 0xffff {
        return Err(HidppError::UnexpectedResponse("onboard profile not in the directory"));
    }
    let bytes = read_sector(dev, sector)?;
    if !verify(&bytes) {
        return Err(HidppError::BadChecksum);
    }
    Ok(ActiveProfile { number, sector, bytes })
}

pub fn read_led<T: Transport>(dev: &mut Device<T>) -> Result<Led> {
    read_active_profile(dev)?.led()
}

/// Grava `led` no perfil ativo. Ordem: ler (CRC válido) → `backup` (erro aborta) → gravar →
/// reler e comparar (se diferir, regrava o original e devolve `VerifyFailed`) → recarregar.
/// Se o LED já é o pedido, não faz nada.
pub fn set_led<T: Transport>(
    dev: &mut Device<T>,
    led: &Led,
    backup: impl FnOnce(&ActiveProfile) -> std::io::Result<()>,
) -> Result<()> {
    let original = read_active_profile(dev)?;
    let new = original.with_led(led);
    if new.bytes == original.bytes {
        return Ok(());
    }
    backup(&original)?;
    write_sector(dev, new.sector, &new.bytes)?;
    if read_sector(dev, new.sector)? != new.bytes {
        write_sector(dev, original.sector, &original.bytes)?;
        return Err(HidppError::VerifyFailed);
    }
    dev.call(ONBOARD_PROFILES, FN_RELOAD, &[0, new.number])?;
    Ok(())
}

/// `$XDG_DATA_HOME/ghub-linux/onboard-backup` (padrão `~/.local/share/...`).
pub fn default_backup_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("ghub-linux/onboard-backup")
}

/// Salva o setor bruto em `dir/<tag>-sector<hhhh>-<unix>.bin`.
pub fn save_backup(dir: &Path, tag: &str, p: &ActiveProfile) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("{tag}-sector{:04x}-{secs}.bin", p.sector));
    std::fs::write(&path, p.bytes)?;
    Ok(path)
}
```

- [ ] **Step 5: Rodar e ver passar**

Run: `RUSTFLAGS="-D warnings" cargo test -p hidpp`
Expected: PASS. Se o `read_sector` falhar por `data[4..]` curto no `MemOnboard` (relatório curto de 7 bytes em `fn4` sem parâmetros), `p[1]` não é lido nesse caso — `fn4` não usa `p`; `fn3` envia 2 parâmetros e cabe no relatório curto (3 parâmetros), então `p[1]` existe.

- [ ] **Step 6: Commit**

```bash
git add crates/hidpp
git commit -m "feat(hidpp): onboard profile read/write with CRC, backup and read-back verification

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `ghubctl led get|set` e validação no hardware

**Files:**
- Modify: `crates/ghubctl/src/main.rs`

**Interfaces:**
- Consumes: `onboard::{read_led, set_led, save_backup, default_backup_dir, Led, ONBOARD_PROFILES}`, `list_logitech`, `Device`, `CANDIDATE_INDICES`, `PING_TIMEOUT` (já existem em `main.rs`).
- Produces: `ghubctl led get` e `ghubctl led set <effect> [--color #RRGGBB] [--period MS] [--intensity N] [--yes]`.

- [ ] **Step 1: Subcomando**

Em `enum Command` acrescentar:

```rust
    /// Lê ou grava o LED do perfil onboard ativo (pare o ghubd antes: `systemctl --user stop ghubd`)
    Led {
        #[command(subcommand)]
        action: LedAction,
    },
```

E, abaixo do enum `Command`:

```rust
#[derive(Subcommand)]
enum LedAction {
    /// Mostra o efeito gravado no perfil ativo
    Get,
    /// Grava um efeito (sem --yes só mostra o que faria)
    Set {
        /// off | fixed | cycle | breathing
        effect: String,
        /// #RRGGBB (fixed e breathing)
        #[arg(long, default_value = "")]
        color: String,
        /// Período em ms (cycle e breathing)
        #[arg(long, default_value_t = 4000)]
        period: u32,
        /// 0 a 100; 0 = 100% (cycle e breathing)
        #[arg(long, default_value_t = 0)]
        intensity: u32,
        /// Confirma a gravação na flash do mouse
        #[arg(long)]
        yes: bool,
    },
}
```

No `match` de `main`: `Command::Led { action } => led(action),` e, ao lado de `use hidpp::features::...`, `onboard`.

- [ ] **Step 2: Implementação**

```rust
fn open_onboard() -> Result<Device<HidrawTransport>, Box<dyn Error>> {
    for info in list_logitech()? {
        let Ok(mut transport) = HidrawTransport::open(&info.path) else { continue };
        for index in CANDIDATE_INDICES {
            let mut dev = Device::new(transport, index);
            dev.set_timeout(PING_TIMEOUT);
            let found = dev.ping().is_ok() && {
                dev.set_timeout(DEFAULT_TIMEOUT);
                dev.discover().is_ok()
            } && dev.feature_index(onboard::ONBOARD_PROFILES).is_ok();
            if found {
                dev.set_timeout(DEFAULT_TIMEOUT);
                return Ok(dev);
            }
            transport = dev.into_transport();
        }
    }
    Err("nenhum dispositivo com perfis onboard (0x8100) encontrado".into())
}

fn led(action: LedAction) -> Result<(), Box<dyn Error>> {
    match action {
        LedAction::Get => {
            let mut dev = open_onboard()?;
            let (effect, color, period, intensity) = onboard::read_led(&mut dev)?.parts();
            println!("effect={effect} color={color} period_ms={period} intensity={intensity}");
            Ok(())
        }
        LedAction::Set { effect, color, period, intensity, yes } => {
            let led = onboard::Led::from_parts(&effect, &color, period, intensity)?;
            if !yes {
                println!("Gravaria {led:?} no perfil ativo. Repita com --yes para gravar na flash do mouse.");
                return Ok(());
            }
            let mut dev = open_onboard()?;
            let dir = onboard::default_backup_dir();
            onboard::set_led(&mut dev, &led, |p| {
                let path = onboard::save_backup(&dir, "ghubctl", p)?;
                println!("cópia de segurança: {}", path.display());
                Ok(())
            })?;
            println!("gravado: {led:?}");
            Ok(())
        }
    }
}
```

- [ ] **Step 3: Compilar e conferir o `--help`**

Run: `RUSTFLAGS="-D warnings" cargo build -p ghubctl && cargo run -q -p ghubctl -- led set fixed --color '#FF0000'`
Expected: imprime "Gravaria Fixed { rgb: [255, 0, 0] } … Repita com --yes" e não toca no mouse. `cargo run -q -p ghubctl -- led set fixed --color red` → erro "cor inválida…".

- [ ] **Step 4: Leitura no mouse real (somente leitura)**

Run: `systemctl --user stop ghubd; cargo run -q -p ghubctl -- led get; systemctl --user start ghubd`
Expected: `effect=cycle color= period_ms=8000 intensity=0` (o estado original).

- [ ] **Step 5: Validação com gravação real — pedir aprovação ao usuário antes**

Dizer ao usuário que vai gravar na flash e pedir o OK explícito. Com o OK, para cada comando abaixo (com `ghubd` parado) rodar, pedir ao usuário que descreva o LED e anotar:

```bash
systemctl --user stop ghubd
cargo run -q -p ghubctl -- led set fixed --color '#00FF00' --yes      # esperado: verde fixo
cargo run -q -p ghubctl -- led set breathing --color '#FF0000' --period 4000 --yes
cargo run -q -p ghubctl -- led set off --yes
cargo run -q -p ghubctl -- led set cycle --period 8000 --yes           # volta ao original
systemctl --user start ghubd
```

Resultado esperado de cada um: o LED muda conforme o efeito. **Decisões a registrar:**
- Respiração: se o LED respirou vermelho, manter `BREATHING_WAVEFORM = 0x00`. Se ficou estático/incorreto, repetir trocando `waveform` em `onboard.rs` (`BREATHING_WAVEFORM`) para `0x01`, `0x02`, regravando; fixar o valor que respira e atualizar a constante e o teste `from_parts_accepts_valid_input_and_normalizes_color` (usa a constante, não o literal).
- Se um efeito (respiração ou desligado) **não funcionar de nenhum jeito**: remover do `LedKind::ALL` (mantendo o variante no enum e no codec) e anotar no doc; a GUI só mostra o que `ALL` lista.
- Medir a duração: `time cargo run -q -p ghubctl -- led set cycle --period 8000 --yes` (a Task 6 usa 15 s de timeout; se passar de ~10 s, avisar).

- [ ] **Step 6: Registrar e commitar**

Acrescentar a `docs/onboard-profiles.md` uma seção "Validação dos efeitos (ghubctl)" com o resultado de cada efeito e o valor final de `BREATHING_WAVEFORM`. Confirmar que o LED voltou ao ciclo original (`ghubctl led get`).

```bash
git add crates/ghubctl crates/hidpp docs/onboard-profiles.md
git commit -m "feat(ghubctl): led get/set on the active onboard profile

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Contrato D-Bus (`ghub-ipc`)

**Files:**
- Modify: `crates/ghub-ipc/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct LedSetting { pub effect: String, pub color: String, pub period_ms: u32, pub intensity: u32 }` (assinatura `(ssuu)`; `Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type`).
  - Proxy: `fn led_effects(&self, id: &str) -> zbus::Result<Vec<String>>`, `fn get_led(&self, id: &str) -> zbus::Result<LedSetting>`, `fn set_led(&self, id: &str, effect: &str, color: &str, period_ms: u32, intensity: u32) -> zbus::Result<()>`.

- [ ] **Step 1: Testes**

No `mod tests` de `ghub-ipc`:

```rust
    #[test]
    fn led_setting_signature_is_stable() {
        assert_eq!(LedSetting::SIGNATURE.to_string(), "(ssuu)");
    }

    #[test]
    fn led_setting_roundtrips() {
        let v = LedSetting { effect: "fixed".into(), color: "#FE8019".into(), period_ms: 0, intensity: 0 };
        assert_eq!(roundtrip(&v), v);
    }
```

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p ghub-ipc` → erro de compilação (`LedSetting`).

- [ ] **Step 3: Implementar**

Depois de `DeviceState`:

```rust
/// LED do perfil onboard ativo. `effect`: off | fixed | cycle | breathing; `color`: `#RRGGBB` ou vazio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct LedSetting {
    pub effect: String,
    pub color: String,
    pub period_ms: u32,
    pub intensity: u32,
}
```

No trait do proxy, depois de `set_polling_rate`:

```rust
    /// Efeitos de LED suportados pelo dispositivo; vazio quando não há suporte.
    fn led_effects(&self, id: &str) -> zbus::Result<Vec<String>>;
    fn get_led(&self, id: &str) -> zbus::Result<LedSetting>;
    fn set_led(&self, id: &str, effect: &str, color: &str, period_ms: u32, intensity: u32) -> zbus::Result<()>;
```

- [ ] **Step 4: Rodar e ver passar** — `RUSTFLAGS="-D warnings" cargo test -p ghub-ipc` → PASS (o resto do workspace só compila após as Tasks 5–6, pois `DaemonService` e `DaemonApi` ainda não implementam os métodos novos; o proxy é só cliente).

- [ ] **Step 5: Commit**

```bash
git add crates/ghub-ipc
git commit -m "feat(ipc): LedSetting and LED methods on the daemon proxy

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 5: `ghubd` — `Mouse`, `Capabilities`, `FakeMouse`

**Files:**
- Modify: `crates/ghubd/src/mouse.rs`, `crates/ghubd/src/testing.rs`

**Interfaces:**
- Consumes: Task 2 (`onboard::{set_led, read_led, save_backup, default_backup_dir, Led, LedKind, ONBOARD_PROFILES}`, `MemOnboard`).
- Produces:
  - `Capabilities.led: Vec<LedKind>` (vazio = sem suporte).
  - `Mouse::led(&mut self) -> MouseResult<Led>` e `Mouse::set_led(&mut self, led: &Led) -> MouseResult<()>`.
  - `HidppMouse::with_backup_dir(self, dir: PathBuf) -> Self`.
  - `FakeState.led: Led`, `led_sets: Vec<Led>`, `FakeState.caps.led`.

- [ ] **Step 1: Testes em `mouse.rs`**

Acrescentar ao `mod tests` de `mouse.rs`:

```rust
    use hidpp::features::onboard::{self, Led, LedKind};
    use hidpp::features::onboard_fake::MemOnboard;

    const CYCLE: Led = Led::Cycle { period_ms: 8000, intensity: 0 };

    fn led_mouse(mem: MemOnboard, dir: &std::path::Path) -> HidppMouse<MemOnboard> {
        let features = HashMap::from([(onboard::ONBOARD_PROFILES, 8u8)]);
        HidppMouse::new(Device::with_features(mem, 1, features)).with_backup_dir(dir.to_path_buf())
    }

    #[test]
    fn led_support_follows_the_onboard_feature() {
        let without = mouse(FakeTransport::new());
        let mut without = without;
        // sem 0x8100 e sem respostas: capabilities falha na leitura de DPI antes; testa só o campo.
        assert!(without.device.feature_index(onboard::ONBOARD_PROFILES).is_err());
        let dir = tempfile::tempdir().unwrap();
        let m = led_mouse(MemOnboard::new(8, &CYCLE), dir.path());
        assert!(m.device.feature_index(onboard::ONBOARD_PROFILES).is_ok());
    }

    #[test]
    fn reads_and_sets_the_led_backing_up_once_per_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = led_mouse(MemOnboard::new(8, &CYCLE), dir.path());
        assert_eq!(m.led(), Ok(CYCLE));
        m.set_led(&Led::Fixed { rgb: [255, 0, 0] }).unwrap();
        m.set_led(&Led::Fixed { rgb: [0, 255, 0] }).unwrap();
        assert_eq!(m.led(), Ok(Led::Fixed { rgb: [0, 255, 0] }));
        let backups = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(backups, 1, "uma cópia por sessão, a do estado original");
        let kept = std::fs::read(std::fs::read_dir(dir.path()).unwrap().next().unwrap().unwrap().path()).unwrap();
        let orig = hidpp::features::onboard_fake::sample_profile_sector(&CYCLE);
        assert_eq!(kept, orig.to_vec());
    }
```

(Ajuste: o primeiro teste deve ficar simples; se o `mouse()` helper existente não expõe `device` como campo acessível, usar `m.device` dentro do módulo de testes — `device` é campo privado do mesmo módulo, acessível aos testes filhos.)

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p ghubd mouse` → erro de compilação (`with_backup_dir`, `led`, `set_led`).

- [ ] **Step 3: Implementar em `mouse.rs`**

Imports: `use hidpp::features::onboard::{self, Led, LedKind}; use std::path::PathBuf;`. Em `Capabilities` acrescentar `pub led: Vec<LedKind>,` (docstring: "Efeitos de LED suportados; vazio sem `0x8100`."). No trait `Mouse`:

```rust
    /// LED gravado no perfil onboard ativo.
    fn led(&mut self) -> MouseResult<Led>;
    /// Grava o LED no perfil onboard ativo (lento: escreve na flash do mouse).
    fn set_led(&mut self, led: &Led) -> MouseResult<()>;
```

`HidppMouse`:

```rust
pub struct HidppMouse<T: Transport> {
    device: Device<T>,
    backup_dir: PathBuf,
    backed_up: bool,
}

impl<T: Transport> HidppMouse<T> {
    pub fn new(device: Device<T>) -> Self {
        HidppMouse { device, backup_dir: onboard::default_backup_dir(), backed_up: false }
    }

    pub fn with_backup_dir(mut self, dir: PathBuf) -> Self {
        self.backup_dir = dir;
        self
    }
    // into_device inalterado
}
```

Em `impl Mouse for HidppMouse`: em `capabilities`, acrescentar ao literal
`led: if self.device.feature_index(onboard::ONBOARD_PROFILES).is_ok() { LedKind::ALL.to_vec() } else { Vec::new() },` e:

```rust
    fn led(&mut self) -> MouseResult<Led> {
        onboard::read_led(&mut self.device)
    }

    fn set_led(&mut self, led: &Led) -> MouseResult<()> {
        let tag = format!("dev{}", self.device.device_index());
        let HidppMouse { device, backup_dir, backed_up } = self;
        onboard::set_led(device, led, |p| {
            if !*backed_up {
                onboard::save_backup(backup_dir, &tag, p)?;
                *backed_up = true;
            }
            Ok(())
        })
    }
```

- [ ] **Step 4: `FakeMouse` em `testing.rs`**

Em `FakeState`: `pub led: Led, pub led_sets: Vec<Led>,` (importar `hidpp::features::onboard::{Led, LedKind}`); em `FakeMouse::new`: `led: Led::Cycle { period_ms: 8000, intensity: 0 }, led_sets: Vec::new(),` e em `caps`: `led: LedKind::ALL.to_vec(),`. Em `impl Mouse for FakeMouse`:

```rust
    fn led(&mut self) -> MouseResult<Led> {
        let s = self.state();
        if s.online && !s.fail_reads { Ok(s.led) } else { Err(HidppError::Timeout) }
    }

    fn set_led(&mut self, led: &Led) -> MouseResult<()> {
        let mut s = self.state();
        if !s.online || s.fail_writes {
            return Err(HidppError::Timeout);
        }
        s.led = *led;
        s.led_sets.push(*led);
        Ok(())
    }
```

Corrigir os demais lugares que constroem `Capabilities { … }` literalmente (o compilador aponta; só `mouse.rs` e `testing.rs`).

- [ ] **Step 5: Rodar e ver passar** — `RUSTFLAGS="-D warnings" cargo test -p ghubd` → PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): Mouse led()/set_led() over the onboard profile, with a once-per-session backup

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 6: `ghubd` — gerente e serviço D-Bus

**Files:**
- Modify: `crates/ghubd/src/manager.rs`, `crates/ghubd/src/service.rs`, `docs/daemon.md`

**Interfaces:**
- Consumes: Task 4 (`LedSetting`), Task 5 (`Mouse::{led,set_led}`, `Capabilities.led`), `Led::{from_parts, parts}`.
- Produces:
  - `Command::LedEffects { id, reply: Sender<Result<Vec<String>, ManagerError>> }`, `Command::GetLed { id, reply: Sender<Result<Led, ManagerError>> }`, `Command::SetLed { id, led: Led, reply: Sender<Result<(), ManagerError>> }`.
  - `Manager::{led_effects, led, set_led}`.
  - D-Bus `LedEffects(s) -> as`, `GetLed(s) -> (ssuu)`, `SetLed(s, s, s, u, u)`.
  - `pub const LED_REPLY_TIMEOUT: Duration = Duration::from_secs(15);` em `service.rs`.

- [ ] **Step 1: Testes do gerente**

No `mod tests` de `manager.rs` (use os helpers `setup_with`/`seeded_store` já existentes):

```rust
    #[test]
    fn led_effects_lists_what_the_device_supports() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        assert_eq!(m.led_effects("dev-1").unwrap(), vec!["off", "fixed", "cycle", "breathing"]);
        mouse.state().caps.led.clear();
        m.rescan();
        assert_eq!(m.led_effects("dev-1").unwrap(), Vec::<String>::new());
        assert!(matches!(m.led_effects("nope"), Err(ManagerError::NotFound(_))));
    }

    #[test]
    fn set_led_writes_emits_and_validates() {
        let (mut m, rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        drain(&rx);
        let red = hidpp::features::onboard::Led::Fixed { rgb: [255, 0, 0] };
        m.set_led("dev-1", red).unwrap();
        assert_eq!(mouse.state().led_sets, vec![red]);
        assert_eq!(m.led("dev-1").unwrap(), red);
        assert!(drain(&rx).contains(&Event::DeviceChanged("dev-1".into())));
    }

    #[test]
    fn set_led_rejects_unsupported_effects_and_offline_devices_without_writing() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        mouse.state().caps.led = vec![hidpp::features::onboard::LedKind::Fixed];
        m.rescan();
        assert!(matches!(m.set_led("dev-1", hidpp::features::onboard::Led::Off), Err(ManagerError::Invalid(_))));
        mouse.state().online = false;
        assert!(matches!(
            m.set_led("dev-1", hidpp::features::onboard::Led::Fixed { rgb: [1, 2, 3] }),
            Err(ManagerError::Offline(_))
        ));
        assert!(mouse.state().led_sets.is_empty());
    }

    #[test]
    fn set_led_reports_a_failed_write_as_a_device_error() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        mouse.state().fail_writes = true;
        assert!(matches!(
            m.set_led("dev-1", hidpp::features::onboard::Led::Off),
            Err(ManagerError::Device(_))
        ));
    }
```

Se `m.rescan()` após `caps.led.clear()` não relê as capacidades (capacidades são lidas ao conectar), reiniciar o cenário em vez de chamar `rescan` de novo: criar o `FakeMouse` já com `caps.led` alterada antes de `setup_with` e ajustar o teste; o objetivo é provar as duas listas.

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p ghubd manager` → compilação (`led_effects`, `set_led`, `led` não existem).

- [ ] **Step 3: Implementar no gerente**

Em `Command` (acima de `Shutdown`):

```rust
    LedEffects { id: String, reply: Sender<Result<Vec<String>, ManagerError>> },
    GetLed { id: String, reply: Sender<Result<Led, ManagerError>> },
    SetLed { id: String, led: Led, reply: Sender<Result<(), ManagerError>> },
```

(`use hidpp::features::onboard::Led;` no topo.) Métodos, ao lado de `set_polling`:

```rust
    /// Efeitos de LED do dispositivo; vazio sem suporte (ou offline).
    pub fn led_effects(&self, id: &str) -> Result<Vec<String>, ManagerError> {
        let idx = self.find(id)?;
        Ok(self.devices[idx]
            .caps
            .as_ref()
            .map(|c| c.led.iter().map(|k| k.name().to_string()).collect())
            .unwrap_or_default())
    }

    pub fn led(&mut self, id: &str) -> Result<Led, ManagerError> {
        let idx = self.find(id)?;
        self.online_caps(idx)?;
        let mouse = self.devices[idx].mouse.as_mut().expect("online");
        mouse.led().map_err(|e| ManagerError::Device(e.to_string()))
    }

    pub fn set_led(&mut self, id: &str, led: Led) -> Result<(), ManagerError> {
        let idx = self.find(id)?;
        let caps = self.online_caps(idx)?;
        if !caps.led.contains(&led.kind()) {
            return Err(ManagerError::Invalid(format!(
                "o efeito \"{}\" não é suportado por este dispositivo",
                led.kind().name()
            )));
        }
        let mouse = self.devices[idx].mouse.as_mut().expect("online");
        mouse.set_led(&led).map_err(|e| ManagerError::Device(e.to_string()))?;
        self.emit(Event::DeviceChanged(id.to_string()));
        Ok(())
    }
```

Em `handle`:

```rust
            Command::LedEffects { id, reply } => {
                let _ = reply.send(self.led_effects(&id));
            }
            Command::GetLed { id, reply } => {
                let _ = reply.send(self.led(&id));
            }
            Command::SetLed { id, led, reply } => {
                let _ = reply.send(self.set_led(&id, led));
            }
```

Atenção: o gerente roda um laço com ciclo de 5 s de `ping`; uma gravação de vários segundos atrasa esse ciclo — aceitável (mesmo padrão das outras escritas).

- [ ] **Step 4: Testes do serviço** (em `service.rs`)

```rust
    #[test]
    fn led_roundtrip_and_validation_over_the_service() {
        let (svc, tx, handle, _d) = running_service();
        assert_eq!(svc.led_effects("dev-1").unwrap(), vec!["off", "fixed", "cycle", "breathing"]);
        let cur = svc.get_led("dev-1").unwrap();
        assert_eq!((cur.effect.as_str(), cur.period_ms), ("cycle", 8000));
        svc.set_led("dev-1", "fixed", "#FE8019", 0, 0).unwrap();
        let cur = svc.get_led("dev-1").unwrap();
        assert_eq!((cur.effect.as_str(), cur.color.as_str()), ("fixed", "#FE8019"));

        let invalid = |r: fdo::Result<()>| matches!(r, Err(fdo::Error::InvalidArgs(_)));
        assert!(invalid(svc.set_led("dev-1", "sparkle", "", 0, 0)));
        assert!(invalid(svc.set_led("dev-1", "fixed", "#GGG", 0, 0)));
        assert!(invalid(svc.set_led("dev-1", "fixed", "", 0, 0)));
        assert!(invalid(svc.set_led("dev-1", "cycle", "", 0, 0)));
        assert!(matches!(svc.get_led("nope"), Err(fdo::Error::UnknownObject(_))));
        // Nada disso mexeu no mouse além da gravação boa.
        assert_eq!(svc.get_led("dev-1").unwrap().effect, "fixed");
        stop(tx, handle);
    }
```

- [ ] **Step 5: Rodar e ver falhar** — `cargo test -p ghubd service` → compilação.

- [ ] **Step 6: Implementar no serviço**

Constante e novo `ask` com timeout próprio:

```rust
/// Gravar o LED escreve na flash do mouse e demora mais que as outras chamadas.
pub const LED_REPLY_TIMEOUT: Duration = Duration::from_secs(15);
```

Refatorar `ask` para `fn ask_for<T>(&self, timeout: Duration, make: …)` e `fn ask(&self, make)` chamando `ask_for(self.timeout, make)`; `ask_result` idem. Para o `SetLed`, usar `ask_for(self.timeout.max(LED_REPLY_TIMEOUT), …)` (assim o teste com `with_timeout` curto continua curto só para as demais; para SetLed o mínimo é 15 s).

Imports: `use ghub_ipc::{DeviceState, DeviceSummary, LedSetting}; use hidpp::features::onboard::Led;`. Métodos:

```rust
    fn led_effects(&self, id: &str) -> fdo::Result<Vec<String>> {
        let id = id.to_string();
        self.ask_result(|reply| Command::LedEffects { id, reply })
    }

    fn get_led(&self, id: &str) -> fdo::Result<LedSetting> {
        let id = id.to_string();
        let led = self.ask_result(|reply| Command::GetLed { id, reply })?;
        let (effect, color, period_ms, intensity) = led.parts();
        Ok(LedSetting { effect: effect.to_string(), color, period_ms: u32::from(period_ms), intensity: u32::from(intensity) })
    }

    fn set_led(&self, id: &str, effect: &str, color: &str, period_ms: u32, intensity: u32) -> fdo::Result<()> {
        let led = Led::from_parts(effect, color, period_ms, intensity).map_err(fdo::Error::InvalidArgs)?;
        let id = id.to_string();
        let (reply_tx, reply_rx) = channel();
        self.tx
            .send(Command::SetLed { id, led, reply: reply_tx })
            .map_err(|_| fdo::Error::Failed("o gerente de dispositivos parou".into()))?;
        reply_rx
            .recv_timeout(self.timeout.max(LED_REPLY_TIMEOUT))
            .map_err(|_| fdo::Error::Failed("o gerente de dispositivos não respondeu a tempo".into()))?
            .map_err(to_fdo)
    }
```

(Mantém os nomes D-Bus `LedEffects`, `GetLed`, `SetLed` gerados pelo `#[interface]` a partir dos nomes em snake_case.)

- [ ] **Step 7: Rodar e ver passar** — `RUSTFLAGS="-D warnings" cargo test --workspace --exclude ghub-gui` → PASS.

- [ ] **Step 8: Documentar e commitar**

Em `docs/daemon.md`, na referência da interface, acrescentar `LedEffects`, `GetLed`, `SetLed` com a semântica: grava no perfil onboard ativo, lento (timeout 15 s), `InvalidArgs` para efeito/cor/período/intensidade inválidos, `Failed` para mouse offline/erro HID++, sinal `DeviceChanged` após gravar, cópia de segurança em `~/.local/share/ghub-linux/onboard-backup/`.

```bash
git add crates/ghubd docs/daemon.md
git commit -m "feat(ghubd): LedEffects/GetLed/SetLed over D-Bus

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 9: Teste de ponta a ponta no mouse (com aprovação do usuário)**

Reinstalar (`./packaging/install-user.sh --no-gui`), subir o `ghubd`, e com a aprovação do usuário:

```bash
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 LedEffects s "<id do dispositivo>"
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 SetLed sssuu "<id>" fixed "#FE8019" 0 0
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 SetLed sssuu "<id>" cycle "" 8000 0
```

(O id sai de `busctl --user call … Devices`.) Esperado: LED laranja e depois ciclo de volta.

---

### Task 7: GUI — cliente, fake e controller

**Files:**
- Modify: `crates/ghub-gui/src/client/mod.rs`, `real.rs`, `fake.rs`, `controller.rs`; `crates/ghub-gui/src/model/state.rs`

**Interfaces:**
- Consumes: `ghub_ipc::LedSetting`, proxy `led_effects/get_led/set_led`.
- Produces:
  - `DaemonApi::{led_effects(&self, id:&str)->Result<Vec<String>,ApiError>, get_led(&self, id:&str)->Result<LedSetting,ApiError>, set_led(&self, id:&str, s:&LedSetting)->Result<(),ApiError>}`.
  - `model::state::LedView { pub effects: Vec<String>, pub current: LedSetting }`, `DeviceView.led: Option<LedView>`, `DeviceView::with_led(self, Option<LedView>) -> Self`.
  - `Command::SetLed(LedSetting)`, `Update::LedDone`.

- [ ] **Step 1: Testes**

Em `controller.rs` (`mod tests`; usar os helpers `rig`, `cmd`, `next_state`, `count` e `wait_for_call` já presentes):

```rust
    #[test]
    fn snapshot_includes_the_led_when_supported() {
        let d = Arc::new(FakeDaemon::new());
        let v = snapshot(&*d, None, None);
        let led = v.device.unwrap().led.expect("LED suportado no fake");
        assert_eq!(led.effects, vec!["off", "fixed", "cycle", "breathing"]);
        assert_eq!(led.current.effect, "cycle");
    }

    #[test]
    fn snapshot_has_no_led_when_the_device_reports_none() {
        let d = Arc::new(FakeDaemon::scenario("no-led"));
        assert!(snapshot(&*d, None, None).device.unwrap().led.is_none());
    }

    #[test]
    fn set_led_writes_refreshes_and_signals_done() {
        let d = Arc::new(FakeDaemon::new());
        let (tx, urx, _h) = rig(d.clone());
        let _ = next_state(&urx); // estado inicial
        cmd(&tx, Command::SetLed(ghub_ipc::LedSetting {
            effect: "fixed".into(), color: "#FF0000".into(), period_ms: 0, intensity: 0,
        }));
        wait_for_call(&d, "set_led");
        assert!(d.calls().contains(&"set_led dev-1 fixed #FF0000 0 0".to_string()));
        let mut saw_done = false;
        let mut led_after = None;
        for _ in 0..6 {
            match urx.recv_timeout(Duration::from_secs(2)).unwrap() {
                Update::LedDone => saw_done = true,
                Update::State(s) => led_after = s.device.and_then(|d| d.led).map(|l| l.current.effect),
                Update::Error(_) => {}
            }
            if saw_done && led_after.as_deref() == Some("fixed") { break; }
        }
        assert!(saw_done, "sempre sinaliza o fim da gravação");
        assert_eq!(led_after.as_deref(), Some("fixed"));
    }

    #[test]
    fn set_led_failure_reports_the_error_and_still_signals_done() {
        let d = Arc::new(FakeDaemon::new());
        d.inner.lock().unwrap().fail_next_write = Some(ApiError::Failed("mouse dormindo".into()));
        let (tx, urx, _h) = rig(d.clone());
        let _ = next_state(&urx);
        cmd(&tx, Command::SetLed(ghub_ipc::LedSetting {
            effect: "off".into(), color: String::new(), period_ms: 0, intensity: 0,
        }));
        let (mut err, mut done) = (false, false);
        for _ in 0..6 {
            match urx.recv_timeout(Duration::from_secs(2)).unwrap() {
                Update::Error(t) if t.contains("mouse dormindo") => err = true,
                Update::LedDone => done = true,
                _ => {}
            }
            if err && done { break; }
        }
        assert!(err && done);
    }
```

(`fail_next_write` é um campo novo do `FakeInner` que só `set_led` consome; evita interferir com as leituras que o `refresh` faz — `fail_next` é consumido pela primeira chamada, que seria uma leitura.)

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p ghub-gui --features fake-daemon controller` → compilação.

- [ ] **Step 3: Implementar**

`client/mod.rs`: `use ghub_ipc::{DeviceState, DeviceSummary, LedSetting};` e no trait:

```rust
    fn led_effects(&self, id: &str) -> Result<Vec<String>, ApiError>;
    fn get_led(&self, id: &str) -> Result<LedSetting, ApiError>;
    fn set_led(&self, id: &str, setting: &LedSetting) -> Result<(), ApiError>;
```

`real.rs` (no `impl DaemonApi for RealDaemon`, junto de `set_polling_rate`):

```rust
    fn led_effects(&self, id: &str) -> Result<Vec<String>, ApiError> {
        self.call(|p| p.led_effects(id))
    }
    fn get_led(&self, id: &str) -> Result<LedSetting, ApiError> {
        self.call(|p| p.get_led(id))
    }
    fn set_led(&self, id: &str, s: &LedSetting) -> Result<(), ApiError> {
        self.call(|p| p.set_led(id, &s.effect, &s.color, s.period_ms, s.intensity))
    }
```

(importar `LedSetting`.)

`fake.rs`: em `FakeInner` acrescentar `pub led: LedSetting, pub led_effects: Vec<String>, pub fail_next_write: Option<ApiError>`; em `FakeDaemon::new`: `led: LedSetting { effect: "cycle".into(), color: String::new(), period_ms: 8000, intensity: 0 }, led_effects: ["off","fixed","cycle","breathing"].map(String::from).to_vec(), fail_next_write: None`; no `scenario`: `"no-led" => i.led_effects.clear(),`. Implementar (as leituras **não** registram em `calls`, para não alterar as listas exatas dos testes existentes):

```rust
    fn led_effects(&self, _id: &str) -> Result<Vec<String>, ApiError> {
        let i = self.inner.lock().unwrap();
        if !i.running { return Err(ApiError::Unavailable); }
        Ok(i.led_effects.clone())
    }
    fn get_led(&self, _id: &str) -> Result<LedSetting, ApiError> {
        let i = self.inner.lock().unwrap();
        if !i.running { return Err(ApiError::Unavailable); }
        Ok(i.led.clone())
    }
    fn set_led(&self, id: &str, s: &LedSetting) -> Result<(), ApiError> {
        let mut i = self.inner.lock().unwrap();
        i.calls.push(format!("set_led {id} {} {} {} {}", s.effect, s.color, s.period_ms, s.intensity));
        if !i.running { return Err(ApiError::Unavailable); }
        if let Some(e) = i.fail_next_write.take() { return Err(e); }
        i.led = s.clone();
        Ok(())
    }
```

`model/state.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedView { pub effects: Vec<String>, pub current: ghub_ipc::LedSetting }
```

Em `DeviceView` acrescentar `pub led: Option<LedView>,`; em `from_state` `led: None,`; e `pub fn with_led(mut self, led: Option<LedView>) -> Self { self.led = led; self }`. Corrigir os literais `DeviceView { … }` nos testes de `props.rs`/`state.rs` com `led: None`.

`controller.rs`: em `Command` acrescentar `SetLed(ghub_ipc::LedSetting)`; em `Update` acrescentar `LedDone`; em `snapshot`, trocar `Ok(st) => v.device = Some(DeviceView::from_state(&st)),` por:

```rust
            Ok(st) => {
                let mut dv = DeviceView::from_state(&st);
                if st.connected {
                    if let Ok(effects) = api.led_effects(&s.id) {
                        if !effects.is_empty() {
                            if let Ok(current) = api.get_led(&s.id) {
                                dv = dv.with_led(Some(LedView { effects, current }));
                            }
                        }
                    }
                }
                v.device = Some(dv);
            }
```

e em `command`, no `match cmd`:

```rust
            Command::SetLed(s) => {
                self.with_device(|c, id| c.api.set_led(&id, &s));
                (self.sink)(Update::LedDone);
            }
```

(O `self.refresh()` que fecha `command` já relê o estado, trazendo o LED novo.) Corrigir o `match` do `backend.rs::apply` para `Update::LedDone` na Task 8.

- [ ] **Step 4: Rodar e ver passar** — `RUSTFLAGS="-D warnings" cargo test -p ghub-gui --features fake-daemon --lib` → PASS (a GUI só compila por inteiro após a Task 8 por causa do `match Update`; se o `-D warnings` acusar o `match` não exaustivo em `backend.rs`, acrescentar provisoriamente `Update::LedDone => {}` e trocar na Task 8).

- [ ] **Step 5: Commit**

```bash
git add crates/ghub-gui
git commit -m "feat(ghub-gui): LED in the client layer, fake daemon and controller

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 8: GUI — modelo de validação, props e ponte Qt

**Files:**
- Create: `crates/ghub-gui/src/model/led.rs`
- Modify: `crates/ghub-gui/src/model/mod.rs`, `props.rs`, `crates/ghub-gui/src/backend.rs`

**Interfaces:**
- Consumes: Task 7 (`DeviceView.led`, `Command::SetLed`, `Update::LedDone`).
- Produces:
  - `model::led::validate(effect:&str, color:&str, period_ms:i32, intensity:i32, supported:&[String]) -> Result<LedSetting, String>`.
  - `Props`: `led_supported: bool, led_effects_json: String, led_effect: String, led_color: String, led_period_ms: i32, led_intensity: i32`.
  - Propriedades Qt `ledSupported, ledEffectsJson, ledEffect, ledColor, ledPeriodMs, ledIntensity, ledBusy` e o invokable `applyLed(effect, color, periodMs, intensity) -> QString`.

- [ ] **Step 1: Testes de `model/led.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<String> {
        ["off", "fixed", "cycle", "breathing"].map(String::from).to_vec()
    }

    #[test]
    fn accepts_each_effect_with_its_fields() {
        assert_eq!(validate("off", "", 0, 0, &all()).unwrap().effect, "off");
        let f = validate("fixed", "#fe8019", 0, 0, &all()).unwrap();
        assert_eq!((f.effect.as_str(), f.color.as_str()), ("fixed", "#FE8019"));
        assert_eq!(validate("cycle", "", 8000, 0, &all()).unwrap().period_ms, 8000);
        assert_eq!(validate("breathing", "#00ff00", 4000, 80, &all()).unwrap().intensity, 80);
    }

    #[test]
    fn rejects_bad_fields_with_a_readable_message() {
        for (e, c, p, i) in [
            ("sparkle", "", 0, 0),
            ("fixed", "", 0, 0),
            ("fixed", "#zzzzzz", 0, 0),
            ("cycle", "", 50, 0),
            ("cycle", "", 70000, 0),
            ("breathing", "#ff0000", 4000, 101),
            ("breathing", "#ff0000", 4000, -1),
        ] {
            let err = validate(e, c, p, i, &all()).unwrap_err();
            assert!(!err.is_empty(), "{e} {c} {p} {i}");
        }
    }

    #[test]
    fn rejects_an_effect_the_device_does_not_list() {
        let only_fixed = vec!["fixed".to_string()];
        assert!(validate("off", "", 0, 0, &only_fixed).is_err());
    }
}
```

- [ ] **Step 2: Rodar e ver falhar** — `cargo test -p ghub-gui --features fake-daemon led` → compilação.

- [ ] **Step 3: Implementar `model/led.rs`** (e `pub mod led;` em `model/mod.rs`)

```rust
//! Validação do formulário de iluminação antes de enviar ao daemon (que valida de novo).

use ghub_ipc::LedSetting;

pub const MIN_PERIOD_MS: i32 = 100;
pub const MAX_PERIOD_MS: i32 = 60_000;

fn normalize_color(color: &str) -> Result<String, String> {
    let hex = color.strip_prefix('#').unwrap_or("");
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(format!("#{}", hex.to_ascii_uppercase()))
    } else {
        Err("Cor inválida: use o formato #RRGGBB.".into())
    }
}

pub fn validate(
    effect: &str,
    color: &str,
    period_ms: i32,
    intensity: i32,
    supported: &[String],
) -> Result<LedSetting, String> {
    if !supported.iter().any(|e| e == effect) {
        return Err("Este efeito não é suportado pelo mouse.".into());
    }
    let wants_color = matches!(effect, "fixed" | "breathing");
    let wants_motion = matches!(effect, "cycle" | "breathing");
    let color = if wants_color { normalize_color(color)? } else { String::new() };
    let (period_ms, intensity) = if wants_motion {
        if !(MIN_PERIOD_MS..=MAX_PERIOD_MS).contains(&period_ms) {
            return Err(format!("O período deve ficar entre {MIN_PERIOD_MS} e {MAX_PERIOD_MS} ms."));
        }
        if !(0..=100).contains(&intensity) {
            return Err("A intensidade deve ficar entre 0 e 100.".into());
        }
        (period_ms as u32, intensity as u32)
    } else {
        (0, 0)
    };
    Ok(LedSetting { effect: effect.into(), color, period_ms, intensity })
}
```

- [ ] **Step 4: Props**

Teste em `props.rs`:

```rust
    #[test]
    fn led_props_follow_the_device() {
        use crate::model::state::LedView;
        let mut v = ViewState::default();
        assert!(!Props::from(&v).led_supported);
        let dev = sample_device().with_led(Some(LedView {
            effects: vec!["fixed".into(), "cycle".into()],
            current: ghub_ipc::LedSetting { effect: "fixed".into(), color: "#FE8019".into(), period_ms: 0, intensity: 0 },
        }));
        v.device = Some(dev);
        let p = Props::from(&v);
        assert!(p.led_supported);
        assert_eq!(p.led_effects_json, "[\"fixed\",\"cycle\"]");
        assert_eq!((p.led_effect.as_str(), p.led_color.as_str()), ("fixed", "#FE8019"));
    }
```

(`sample_device()` = o helper de dispositivo que os testes de `props.rs` já usam; se o helper tiver outro nome, usar o existente.) Implementar: campos novos em `Props` (`led_supported: bool, led_effects_json: String, led_effect: String, led_color: String, led_period_ms: i32, led_intensity: i32`), valor padrão `led_effects_json: "[]".into()` no construtor, e dentro de `if let Some(d) = &v.device`:

```rust
            if let Some(l) = &d.led {
                p.led_supported = true;
                p.led_effects_json = json_strings(&l.effects);
                p.led_effect = l.current.effect.clone();
                p.led_color = l.current.color.clone();
                p.led_period_ms = l.current.period_ms as i32;
                p.led_intensity = l.current.intensity as i32;
            }
```

- [ ] **Step 5: Ponte Qt (`backend.rs`)**

Na `bridge`: propriedades

```rust
        #[qproperty(bool, led_supported, cxx_name = "ledSupported")]
        #[qproperty(QString, led_effects_json, cxx_name = "ledEffectsJson")]
        #[qproperty(QString, led_effect, cxx_name = "ledEffect")]
        #[qproperty(QString, led_color, cxx_name = "ledColor")]
        #[qproperty(i32, led_period_ms, cxx_name = "ledPeriodMs")]
        #[qproperty(i32, led_intensity, cxx_name = "ledIntensity")]
        #[qproperty(bool, led_busy, cxx_name = "ledBusy")]
```

e o invokable

```rust
        #[qinvokable]
        #[cxx_name = "applyLed"]
        fn apply_led(self: Pin<&mut Backend>, effect: QString, color: QString, period_ms: i32, intensity: i32) -> QString;
```

Em `BackendRust`: campos `led_supported: bool, led_effects_json: QString, led_effect: QString, led_color: QString, led_period_ms: i32, led_intensity: i32, led_busy: bool`; em `Default`: `led_supported: p.led_supported, led_effects_json: QString::from(p.led_effects_json.as_str()), led_effect: QString::from(p.led_effect.as_str()), led_color: QString::from(p.led_color.as_str()), led_period_ms: p.led_period_ms, led_intensity: p.led_intensity, led_busy: false,`. Em `apply`, depois dos outros `set_*` do `Update::State`:

```rust
                self.as_mut().set_led_supported(p.led_supported);
                self.as_mut().set_led_effects_json(QString::from(p.led_effects_json.as_str()));
                self.as_mut().set_led_effect(QString::from(p.led_effect.as_str()));
                self.as_mut().set_led_color(QString::from(p.led_color.as_str()));
                self.as_mut().set_led_period_ms(p.led_period_ms);
                self.as_mut().set_led_intensity(p.led_intensity);
```

e um braço `Update::LedDone => self.as_mut().set_led_busy(false),`. O invokable:

```rust
    fn apply_led(mut self: Pin<&mut Self>, effect: QString, color: QString, period_ms: i32, intensity: i32) -> QString {
        let supported = self.rust().view.device.as_ref()
            .and_then(|d| d.led.as_ref().map(|l| l.effects.clone()))
            .unwrap_or_default();
        match crate::model::led::validate(&effect.to_string(), &color.to_string(), period_ms, intensity, &supported) {
            Ok(setting) => {
                self.as_mut().set_led_busy(true);
                self.send(Command::SetLed(setting));
                QString::default()
            }
            Err(e) => QString::from(e.as_str()),
        }
    }
```

- [ ] **Step 6: Rodar e ver passar** — `RUSTFLAGS="-D warnings" cargo test -p ghub-gui --features fake-daemon --lib` e `cargo build -p ghub-gui --features "qt fake-daemon"` (usar as features que o `docs/gui.md` indica para compilar com Qt) → PASS/compila.

- [ ] **Step 7: Commit**

```bash
git add crates/ghub-gui
git commit -m "feat(ghub-gui): LED form validation, props and Qt bridge

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 9: GUI — página Iluminação, aba, smoke e capturas

**Files:**
- Create: `crates/ghub-gui/qml/components/LedPreview.qml`, `crates/ghub-gui/qml/pages/Lighting.qml`
- Modify: `crates/ghub-gui/build.rs`, `qml/AppWindow.qml`, `qml/Smoke.qml`, `tests/smoke.rs`

**Interfaces:**
- Consumes: propriedades/invokable da Task 8; componentes `Card`, `PillButton`, `Theme`, `MouseArt`.
- Produces: página `Lighting` (índice 3 do `StackLayout`); funções do `AppWindow` para o smoke: `lightingApplyEnabled()`, `lightingSelect(effect)`, `lightingApply()`, `lightingBusyText()`.

- [ ] **Step 1: Smoke que falha primeiro**

Em `Smoke.qml`, acrescentar ao `switch (root.scenario)`:

```js
        case "lighting": {
            // aba habilitada, formulário reflete o LED do daemon e Aplicar grava
            const w = root.win
            if (!b.hasDevice || !b.ledSupported || w.pageCount !== 4) return false
            w.goTo(3)
            if (w.currentIndex !== 3 || !w.navTab(3).enabled) return false
            if (b.ledEffect !== "cycle" || w.lightingApplyEnabled()) return false   // sem mudança: Aplicar desabilitado
            w.lightingSelect("fixed")
            if (!w.lightingApplyEnabled()) return false
            w.lightingApply()
            return b.ledBusy === true || b.ledEffect === "fixed"
        }
        case "lighting-hidden":
            // sem suporte no mouse: a aba some e a página não é alcançável
            return b.hasDevice && !b.ledSupported && !root.win.navTab(3).visible
```

Em `tests/smoke.rs`:

```rust
#[test]
fn lighting_page_reflects_and_applies_the_led() {
    assert_eq!(run("lighting", Some("running")), 0);
}

#[test]
fn counter_proof_lighting_fails_when_the_mouse_has_no_led() {
    assert_eq!(run("lighting", Some("no-led")), 7);
}

#[test]
fn lighting_tab_is_hidden_when_the_mouse_has_no_led() {
    assert_eq!(run("lighting-hidden", Some("no-led")), 0);
}

#[test]
fn counter_proof_lighting_hidden_fails_when_the_mouse_has_a_led() {
    assert_eq!(run("lighting-hidden", Some("running")), 7);
}
```

Run: `cargo test -p ghub-gui --features "qt fake-daemon" --test smoke lighting` → FAIL (códigos 7: página inexistente).

- [ ] **Step 2: `LedPreview.qml`** (prévia do logo aceso; segue o mock)

```qml
import QtQuick
import org.ghublinux.gui

// Brilho do logo do mouse: cor fixa, respiração (pulsa) ou ciclo (gira o matiz).
Item {
    id: root
    property string effect: "fixed"
    property color color: Theme.accent
    property int periodMs: 4000
    implicitWidth: 120
    implicitHeight: 120

    property real phase: 0
    NumberAnimation on phase {
        running: root.effect === "breathing" || root.effect === "cycle"
        from: 0; to: 1; loops: Animation.Infinite
        duration: Math.max(500, root.periodMs)
    }

    readonly property color shown: {
        if (root.effect === "off") return "transparent"
        if (root.effect === "cycle") return Qt.hsla(root.phase, 0.9, 0.55, 1)
        return root.color
    }
    readonly property real level: root.effect === "breathing"
                                  ? 0.25 + 0.75 * (0.5 - 0.5 * Math.cos(2 * Math.PI * root.phase))
                                  : (root.effect === "off" ? 0 : 1)

    Rectangle {
        anchors.centerIn: parent
        width: parent.width; height: width; radius: width / 2
        color: root.shown
        opacity: 0.25 * root.level
    }
    Rectangle {
        anchors.centerIn: parent
        width: parent.width * 0.62; height: width; radius: width / 2
        color: root.shown
        opacity: 0.4 * root.level
    }
    Rectangle {
        anchors.centerIn: parent
        width: parent.width * 0.3; height: width; radius: width / 2
        color: root.effect === "off" ? Theme.hi : root.shown
        border.color: root.effect === "off" ? Theme.border : root.shown
        border.width: 2
        opacity: root.effect === "off" ? 1 : 0.4 + 0.6 * root.level
    }
}
```

- [ ] **Step 3: `Lighting.qml`**

```qml
import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic
import org.ghublinux.gui

Item {
    id: page
    required property var backend

    readonly property var effects: JSON.parse(backend.ledEffectsJson)
    readonly property var labels: ({ "fixed": "Fixo", "breathing": "Respiração", "cycle": "Ciclo de cores", "off": "Desligado" })
    readonly property var palette: ["#fe8019", "#fabd2f", "#b8bb26", "#8ec07c", "#83a598", "#d3869b", "#fb4934", "#ebdbb2"]
    readonly property bool usable: backend.connected && backend.ledSupported && !backend.ledBusy

    // Estado editado (inicia no do mouse; volta a ele quando o daemon muda).
    property string effect: backend.ledEffect
    property string color: backend.ledColor !== "" ? backend.ledColor : "#FE8019"
    property int periodMs: backend.ledPeriodMs > 0 ? backend.ledPeriodMs : 4000
    property int intensity: backend.ledIntensity
    property string formError: ""

    readonly property bool wantsColor: effect === "fixed" || effect === "breathing"
    readonly property bool wantsMotion: effect === "cycle" || effect === "breathing"
    readonly property bool dirty: effect !== backend.ledEffect
        || (wantsColor && color.toUpperCase() !== backend.ledColor.toUpperCase())
        || (wantsMotion && (periodMs !== backend.ledPeriodMs || intensity !== backend.ledIntensity))
    readonly property bool applyEnabled: usable && dirty

    Connections {
        target: page.backend
        function onLedEffectChanged() { page.effect = page.backend.ledEffect }
        function onLedColorChanged() { if (page.backend.ledColor !== "") page.color = page.backend.ledColor }
        function onLedPeriodMsChanged() { if (page.backend.ledPeriodMs > 0) page.periodMs = page.backend.ledPeriodMs }
        function onLedIntensityChanged() { page.intensity = page.backend.ledIntensity }
    }

    function select(e) { page.effect = e }
    function apply() {
        page.formError = page.backend.applyLed(page.effect, page.color, page.periodMs, page.intensity)
    }
    function busyText() { return backend.ledBusy ? "Gravando no mouse…" : "" }

    RowLayout {
        anchors { fill: parent; margins: Theme.pad }
        spacing: Theme.gap

        Card {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            LedPreview {
                Layout.alignment: Qt.AlignHCenter | Qt.AlignVCenter
                Layout.preferredWidth: 220
                Layout.preferredHeight: 220
                effect: page.effect
                color: page.color
                periodMs: page.periodMs
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            Layout.preferredWidth: 1
            spacing: Theme.gap

            Card {
                Layout.fillWidth: true
                heading: "Efeito"
                subtitle: "Gravado no perfil ativo do mouse."
                Flow {
                    Layout.fillWidth: true
                    spacing: 8
                    Repeater {
                        model: ["fixed", "breathing", "cycle", "off"].filter(e => page.effects.indexOf(e) >= 0)
                        delegate: PillButton {
                            required property string modelData
                            text: page.labels[modelData]
                            primary: page.effect === modelData
                            enabled: page.usable
                            onClicked: page.select(modelData)
                        }
                    }
                }
            }

            Card {
                Layout.fillWidth: true
                visible: page.wantsColor
                heading: "Cor"
                subtitle: "Escolha uma da paleta ou digite #RRGGBB."
                Flow {
                    Layout.fillWidth: true
                    spacing: 10
                    Repeater {
                        model: page.palette
                        delegate: Rectangle {
                            required property string modelData
                            width: 38; height: 38; radius: 10
                            color: modelData
                            border.width: 2
                            border.color: page.color.toLowerCase() === modelData ? Theme.fg : Theme.line
                            MouseArea {
                                anchors.fill: parent
                                enabled: page.usable
                                onClicked: page.color = parent.modelData
                            }
                        }
                    }
                }
                TextField {
                    Layout.topMargin: 14
                    Layout.preferredWidth: 140
                    enabled: page.usable
                    text: page.color
                    font.family: Theme.mono
                    onEditingFinished: page.color = text
                }
            }

            Card {
                Layout.fillWidth: true
                visible: page.wantsMotion
                heading: "Velocidade e intensidade"
                subtitle: "Período do efeito em ms e brilho (0 = 100%)."
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 16
                    Slider {
                        Layout.fillWidth: true
                        from: 500; to: 20000; stepSize: 100
                        enabled: page.usable
                        value: page.periodMs
                        onMoved: page.periodMs = Math.round(value)
                    }
                    Text { text: page.periodMs + " ms"; color: Theme.fg; font.family: Theme.mono }
                }
                RowLayout {
                    Layout.fillWidth: true
                    spacing: 16
                    Slider {
                        Layout.fillWidth: true
                        from: 0; to: 100; stepSize: 5
                        enabled: page.usable
                        value: page.intensity
                        onMoved: page.intensity = Math.round(value)
                    }
                    Text { text: page.intensity + "%"; color: Theme.fg; font.family: Theme.mono }
                }
            }

            Item { Layout.fillHeight: true }

            RowLayout {
                Layout.fillWidth: true
                spacing: 12
                Text {
                    Layout.fillWidth: true
                    text: page.formError !== "" ? page.formError
                          : (page.backend.ledBusy ? "Gravando no mouse…"
                             : "Ao aplicar, a mudança é gravada no mouse.")
                    color: page.formError !== "" ? Theme.bad : Theme.muted
                    wrapMode: Text.WordWrap
                    font.family: Theme.ui
                    font.pixelSize: 13
                }
                PillButton {
                    text: "Aplicar"
                    primary: true
                    enabled: page.applyEnabled
                    onClicked: page.apply()
                }
            }
        }
    }
}
```

- [ ] **Step 4: Ligar na janela**

`build.rs`: acrescentar `QmlFile::from("qml/components/LedPreview.qml"),` e `QmlFile::from("qml/pages/Lighting.qml"),`.

`AppWindow.qml`:
- `readonly property int pageCount: 4`
- `function navTab(i) { return [nav0, nav1, nav2, nav3][i] }`
- funções para o smoke: `function lightingApplyEnabled() { return lighting.applyEnabled }`, `function lightingSelect(e) { lighting.select(e) }`, `function lightingApply() { lighting.apply() }`, `function lightingBusyText() { return lighting.busyText() }`
- substituir a aba desabilitada: `NavTab { id: nav3; text: "Iluminação"; visible: backend.ledSupported; enabled: win.pagesVisible; checked: win.currentIndex === 3; onClicked: win.goTo(3) }`
- no `StackLayout`, depois de `Settings`: `Lighting { id: lighting; backend: win.backend }`
- fallback: `Connections { target: backend; function onLedSupportedChanged() { if (!backend.ledSupported && win.currentIndex === 3) win.goTo(0) } }`

Atualizar o smoke `pages` (`w.pageCount !== 3` → `4`, e o laço `i < 3` → `i < 3` mantém só as três primeiras, ou `i < 4` se o `running` tem LED — usar `i < 4`) e, em `buildPlan`/`pageNames`, incluir `"iluminacao"` como 4º nome com o laço `i < 4`; ajustar a contagem esperada de PNGs em `tests/smoke.rs::every_page_renders_to_a_png_at_both_sizes` (o teste conta arquivos; incluir a página nova: 4 páginas × 2 tamanhos + menu).

- [ ] **Step 5: Rodar e ver passar**

Run: `RUSTFLAGS="-D warnings" cargo test -p ghub-gui --features "qt fake-daemon"` (as features exatas estão em `docs/gui.md`; usar as que a suíte atual usa).
Expected: PASS, incluindo as quatro contraprovas novas e os smokes antigos ajustados.

- [ ] **Step 6: Olhar a tela**

Run: `cargo run -p ghub-gui --features "qt fake-daemon" -- --fake running` e conferir a aba Iluminação contra o mock (prévia, botões de efeito, paleta, hex, sliders, Aplicar). Gerar as capturas com `--smoke shots --shots <dir> --fake running` e abrir o PNG da página nova em 1280×800 e 900×600. Corrigir espaçamentos/cores que destoem do mock.

- [ ] **Step 7: Commit**

```bash
git add crates/ghub-gui
git commit -m "feat(ghub-gui): Iluminação tab with effect, color, speed/intensity and Apply

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Docs, spec e verificação final no hardware

**Files:**
- Modify: `docs/gui.md`, `docs/gui-verification.md`, `docs/superpowers/specs/2026-10-13-iluminacao-v1.1-design.md`, `docs/superpowers/specs/2026-10-12-ghub-linux-design.md`

- [ ] **Step 1: Documentação**

- `docs/gui.md`: descrever a aba Iluminação, o cenário fake `no-led`, e os cenários de smoke `lighting` e `lighting-hidden`.
- `docs/gui-verification.md`, nova seção "Iluminação" com itens: aba bate com o mock; Aplicar fica desabilitado sem mudança; Fixo (vermelho) acende no mouse; Ciclo e Respiração mudam o LED; Desligado apaga; erro com mouse dormindo mostra mensagem e sai de "Gravando…"; a cópia de segurança aparece em `~/.local/share/ghub-linux/onboard-backup/` só na primeira gravação da sessão; **ao final, restaurar o ciclo de cores original** (`ghubctl led set cycle --period 8000 --yes` com `ghubd` parado, ou pela aba).
- Spec `2026-10-13-iluminacao-v1.1-design.md`: trocar o item "DeviceState ganha `led: Option<LedInfo>`" por "método `LedEffects`" (desvio 1) e anotar o timeout de 15 s e o índice 3 da página. Spec geral `2026-10-12-…-design.md` §5.4: trocar "Iluminação e Botões entram como abas desabilitadas" por "Iluminação habilitada na v1.1; Botões desabilitada até a v1.2".

- [ ] **Step 2: Verificação completa automatizada**

Run:
```bash
RUSTFLAGS="-D warnings" cargo test --workspace --exclude ghub-gui
RUSTFLAGS="-D warnings" cargo test -p ghub-gui --features "qt fake-daemon"
cargo clippy --workspace --all-targets 2>&1 | tail -5
```
Expected: tudo PASS, sem warnings.

- [ ] **Step 3: Instalar e verificar no hardware (com aprovação do usuário a cada gravação)**

```bash
./packaging/install-user.sh
systemctl --user restart ghubd
```

Abrir a GUI (pela bandeja) e, com o OK do usuário para cada gravação: Fixo vermelho → Aplicar; Respiração; Ciclo; Desligado; voltar a Ciclo 8000 ms. Para cada passo o usuário confirma o que vê. Confirmar que existe exatamente **uma** cópia nova em `~/.local/share/ghub-linux/onboard-backup/` por sessão do daemon. Ao fim o LED deve estar em Ciclo 8000 ms (`ghubctl led get` com o `ghubd` parado).

- [ ] **Step 4: Commit e fechamento**

```bash
git add docs
git commit -m "docs: lighting tab, verification checklist and spec updates

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Depois, usar `superpowers:finishing-a-development-branch` para decidir o merge/push.

---

## Self-Review

**Spec coverage:** §4 módulo `onboard` → Tasks 1–2; salvaguardas 1–4 → Task 2 (CRC inválido, backup uma vez por sessão no `HidppMouse` — Task 5, leitura de volta + restauração, só bytes de LED + CRC no teste `with_led_changes_only…`); §5 daemon/D-Bus → Tasks 4–6 (incl. timeout de 15 s, `InvalidArgs`, `DeviceChanged`, falha de gravação como erro na GUI); §6 GUI → Tasks 7–9; §7 testes → em cada task + hardware em Tasks 3, 6 (step 9) e 10; §8 ordem → Tasks 1–10 seguem a ordem da spec (a `ghubctl` fica na Task 3, antes do daemon). Notificação por `org.freedesktop.Notifications` para falha de gravação (spec §5): **não** implementada — a GUI mostra o erro (Toast); desvio aceito porque a gravação só parte da GUI aberta. Registrado aqui; anotar na spec no Step 1 da Task 10.
**Placeholders:** o valor de `BREATHING_WAVEFORM` e a lista final de `LedKind::ALL` dependem da validação no hardware (Task 3, Step 5), que é um passo explícito com regra de decisão, não um TBD.
**Consistência de tipos:** `Led::{from_parts, parts}`, `LedKind::{ALL, name}`, `ActiveProfile`, `set_led(dev, &Led, backup)`, `Mouse::set_led(&Led)`, `Command::{LedEffects, GetLed, SetLed}`, `LedSetting` e `Update::LedDone` aparecem com as mesmas assinaturas em todas as tasks.
