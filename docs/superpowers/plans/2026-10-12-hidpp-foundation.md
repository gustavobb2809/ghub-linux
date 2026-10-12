# hidpp Foundation + Hardware Probe Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Criar o workspace Cargo, a biblioteca `hidpp` (protocolo HID++ 2.0 testável sem hardware) e a CLI `ghubctl probe` que lista, em modo somente leitura, as features reais do G703 Lightspeed.

**Architecture:** `hidpp` é uma biblioteca sem D-Bus e sem Qt. Um trait `Transport` isola o I/O (`HidrawTransport` real, `FakeTransport` nos testes). `Device<T>` envia requisições, ignora notificações intercaladas, descobre features (IRoot/IFeatureSet) e as expõe por módulos tipados (bateria, DPI, polling). `ghubctl` é o primeiro consumidor e valida o protocolo no hardware.

**Tech Stack:** Rust 2021 (cargo 1.98.1), `thiserror` 2, `libc` 0.2, `clap` 4 (derive). Sem dependências de Qt/D-Bus neste plano.

**Spec:** `docs/superpowers/specs/2026-10-12-ghub-linux-design.md` (seções 5.1 e 10, passos 1 e 2). Este é o **Plano 1 de 4**. Os próximos (daemon `ghubd`, bandeja + autostart, GUI) dependem do resultado da Task 7.

## Global Constraints

- Rust edição 2021, workspace Cargo com `resolver = "2"`.
- Uma feature HID++ só é usada se o dispositivo a anunciar (`FeatureNotSupported` caso contrário).
- `ghubctl probe` **nunca escreve** no dispositivo; só lê.
- Nenhum `sudo` é executado pelo plano. A regra udev da Task 7 é instalada pelo próprio usuário, com autorização explícita.
- Os IDs de feature deste plano (`0x1000`, `0x1004`, `0x2201`, `0x8060`) vêm de conhecimento público do protocolo e são **hipóteses até a Task 7 confirmar no G703**.
- Nenhum arquivo de licença é criado (decisão em aberto na spec, seção 11). `publish = false` no workspace.
- Testes unitários não dependem de hardware nem de `/dev/hidraw*`.

## Review Focus

Entradas e condições que a spec implica mas nenhum teste óbvio cobriria, da mais provável para a menos:

1. Dispositivo dormindo ou sem resposta: `request` devolve `Timeout` em vez de travar (Task 3).
2. Notificações não solicitadas (ex.: bateria) chegando entre a requisição e a resposta: devem ser ignoradas, não confundidas com a resposta (Task 3).
3. Relatório malformado ou de outra interface (curto, ID desconhecido) na leitura: ignorado no loop, nunca `panic` (Tasks 2 e 3).
4. Feature que o dispositivo não anuncia: `FeatureNotSupported`, sem indexar fora do mapa (Tasks 3 e 4).
5. Lista de DPI em formato de faixa (`min, 0xE000|passo, max`) versus lista explícita; taxa de polling inválida (ex.: 333 Hz) rejeitada antes de escrever (Task 4).

---

### Task 1: Workspace e esqueleto dos crates

**Files:**
- Create: `Cargo.toml`
- Create: `.gitignore`
- Create: `crates/hidpp/Cargo.toml`
- Create: `crates/hidpp/src/lib.rs`
- Create: `crates/ghubctl/Cargo.toml`
- Create: `crates/ghubctl/src/main.rs`
- Create: `README.md`

**Interfaces:**
- Produces: workspace com membros `hidpp` (lib) e `ghubctl` (bin).

- [ ] **Step 1: Criar o workspace**

`Cargo.toml`:

```toml
[workspace]
resolver = "2"
members = ["crates/hidpp", "crates/ghubctl"]

[workspace.package]
version = "0.1.0"
edition = "2021"
publish = false
```

`.gitignore`:

```
/target
```

`README.md`:

```markdown
# ghub-linux

Alternativa nativa para Linux ao Logitech G HUB. Projeto em desenvolvimento.

Design: `docs/superpowers/specs/2026-10-12-ghub-linux-design.md`.
```

- [ ] **Step 2: Criar a biblioteca com um teste de fumaça**

`crates/hidpp/Cargo.toml`:

```toml
[package]
name = "hidpp"
version.workspace = true
edition.workspace = true
publish.workspace = true

[dependencies]
thiserror = "2"
libc = "0.2"
```

`crates/hidpp/src/lib.rs`:

```rust
//! Cliente HID++ 2.0 para dispositivos Logitech.

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_builds() {
        assert_eq!(2 + 2, 4);
    }
}
```

- [ ] **Step 3: Criar o binário `ghubctl` mínimo**

`crates/ghubctl/Cargo.toml`:

```toml
[package]
name = "ghubctl"
version.workspace = true
edition.workspace = true
publish.workspace = true

[dependencies]
hidpp = { path = "../hidpp" }
clap = { version = "4", features = ["derive"] }
```

`crates/ghubctl/src/main.rs`:

```rust
fn main() {
    println!("ghubctl");
}
```

- [ ] **Step 4: Verificar**

Run: `cargo test --workspace`
Expected: `test tests::workspace_builds ... ok`, sem erros de compilação.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml .gitignore README.md crates
git commit -m "chore: scaffold cargo workspace with hidpp and ghubctl"
```

---

### Task 2: Erros e codificação de relatórios HID++

**Files:**
- Create: `crates/hidpp/src/error.rs`
- Create: `crates/hidpp/src/report.rs`
- Modify: `crates/hidpp/src/lib.rs`

**Interfaces:**
- Produces:
  - `HidppError` (enum) com variantes `Malformed(&'static str)`, `Timeout`, `Protocol { feature_index: u8, code: u8 }`, `LegacyProtocol { code: u8 }`, `FeatureNotSupported(u16)`, `UnexpectedResponse(&'static str)`, `InvalidArgument(&'static str)`, `Io(String)`.
  - `type Result<T> = std::result::Result<T, HidppError>`.
  - `Report { device_index: u8, feature_index: u8, function_byte: u8, params: Vec<u8> }`.
  - `Report::request(device_index: u8, feature_index: u8, function: u8, params: &[u8]) -> Report`, `encode(&self) -> Result<Vec<u8>>`, `decode(raw: &[u8]) -> Result<Report>`, `function(&self) -> u8`, `sw_id(&self) -> u8`, `as_error(&self) -> Option<HidppError>`.
  - Constantes: `REPORT_SHORT = 0x10`, `REPORT_LONG = 0x11`, `SHORT_LEN = 7`, `LONG_LEN = 20`, `SW_ID = 0x01`.

- [ ] **Step 1: Escrever os testes que falham**

Substituir `crates/hidpp/src/lib.rs`:

```rust
//! Cliente HID++ 2.0 para dispositivos Logitech.

pub mod error;
pub mod report;

pub use error::{HidppError, Result};
```

Criar `crates/hidpp/src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum HidppError {
    #[error("relatório HID++ malformado: {0}")]
    Malformed(&'static str),
    #[error("tempo esgotado esperando o dispositivo")]
    Timeout,
    #[error("erro HID++ 2.0 0x{code:02x} na feature de índice 0x{feature_index:02x}")]
    Protocol { feature_index: u8, code: u8 },
    #[error("erro HID++ 1.0 0x{code:02x} (dispositivo não fala HID++ 2.0)")]
    LegacyProtocol { code: u8 },
    #[error("o dispositivo não suporta a feature 0x{0:04x}")]
    FeatureNotSupported(u16),
    #[error("resposta inesperada: {0}")]
    UnexpectedResponse(&'static str),
    #[error("argumento inválido: {0}")]
    InvalidArgument(&'static str),
    #[error("erro de E/S: {0}")]
    Io(String),
}

impl From<std::io::Error> for HidppError {
    fn from(e: std::io::Error) -> Self {
        HidppError::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, HidppError>;
```

Criar `crates/hidpp/src/report.rs` somente com os testes (a implementação vem no Step 3):

```rust
use crate::error::{HidppError, Result};

#[cfg(test)]
mod tests {
    use super::*;

    fn padded(mut v: Vec<u8>, len: usize) -> Vec<u8> {
        v.resize(len, 0);
        v
    }

    #[test]
    fn encodes_short_request() {
        let r = Report::request(0x01, 0x00, 1, &[0x00, 0x00, 0xA5]);
        assert_eq!(
            r.encode().unwrap(),
            vec![0x10, 0x01, 0x00, 0x11, 0x00, 0x00, 0xA5]
        );
    }

    #[test]
    fn encodes_long_when_params_exceed_three() {
        let r = Report::request(0x01, 0x02, 3, &[1, 2, 3, 4]);
        let out = r.encode().unwrap();
        assert_eq!(out.len(), 20);
        assert_eq!(out[0], 0x11);
        assert_eq!(&out[..8], &[0x11, 0x01, 0x02, 0x31, 1, 2, 3, 4]);
    }

    #[test]
    fn encode_rejects_more_than_sixteen_params() {
        let r = Report::request(0x01, 0x02, 0, &[0u8; 17]);
        assert_eq!(
            r.encode(),
            Err(HidppError::Malformed("too many parameters"))
        );
    }

    #[test]
    fn decode_long_roundtrip() {
        let raw = padded(vec![0x11, 0x02, 0x05, 0x21, 9, 8, 7], 20);
        let r = Report::decode(&raw).unwrap();
        assert_eq!(r.device_index, 0x02);
        assert_eq!(r.feature_index, 0x05);
        assert_eq!(r.function(), 2);
        assert_eq!(r.sw_id(), 1);
        assert_eq!(&r.params[..3], &[9, 8, 7]);
        assert_eq!(r.params.len(), 16);
    }

    #[test]
    fn decode_rejects_truncated_unknown_and_empty() {
        assert_eq!(
            Report::decode(&[0x11, 1, 0, 0x11]),
            Err(HidppError::Malformed("truncated report"))
        );
        assert_eq!(
            Report::decode(&[0x20, 0, 0, 0, 0, 0, 0]),
            Err(HidppError::Malformed("unknown report id"))
        );
        assert_eq!(
            Report::decode(&[]),
            Err(HidppError::Malformed("empty report"))
        );
    }

    #[test]
    fn classifies_hidpp2_error() {
        let raw = padded(vec![0x11, 0x01, 0xFF, 0x04, 0x21, 0x05], 20);
        let r = Report::decode(&raw).unwrap();
        assert_eq!(
            r.as_error(),
            Some(HidppError::Protocol {
                feature_index: 4,
                code: 5
            })
        );
    }

    #[test]
    fn classifies_legacy_error() {
        let r = Report::decode(&[0x10, 0x01, 0x8F, 0x00, 0x11, 0x01, 0x00]).unwrap();
        assert_eq!(r.as_error(), Some(HidppError::LegacyProtocol { code: 1 }));
    }

    #[test]
    fn normal_report_is_not_an_error() {
        let r = Report::decode(&[0x10, 0x01, 0x04, 0x11, 0, 0, 0]).unwrap();
        assert_eq!(r.as_error(), None);
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p hidpp`
Expected: FAIL com erro de compilação (`Report` não definido).

- [ ] **Step 3: Implementar**

Inserir em `crates/hidpp/src/report.rs`, acima do bloco `#[cfg(test)]` (manter o `use` do topo):

```rust
pub const REPORT_SHORT: u8 = 0x10;
pub const REPORT_LONG: u8 = 0x11;
pub const SHORT_LEN: usize = 7;
pub const LONG_LEN: usize = 20;
/// ID de software enviado nas requisições (nibble baixo do byte de função; nunca 0).
pub const SW_ID: u8 = 0x01;

const HIDPP2_ERROR_INDEX: u8 = 0xFF;
const HIDPP1_ERROR_INDEX: u8 = 0x8F;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub device_index: u8,
    pub feature_index: u8,
    /// Função no nibble alto, ID de software no baixo.
    pub function_byte: u8,
    pub params: Vec<u8>,
}

impl Report {
    pub fn request(device_index: u8, feature_index: u8, function: u8, params: &[u8]) -> Self {
        Report {
            device_index,
            feature_index,
            function_byte: (function << 4) | SW_ID,
            params: params.to_vec(),
        }
    }

    pub fn function(&self) -> u8 {
        self.function_byte >> 4
    }

    pub fn sw_id(&self) -> u8 {
        self.function_byte & 0x0F
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let (id, len) = if self.params.len() <= SHORT_LEN - 4 {
            (REPORT_SHORT, SHORT_LEN)
        } else if self.params.len() <= LONG_LEN - 4 {
            (REPORT_LONG, LONG_LEN)
        } else {
            return Err(HidppError::Malformed("too many parameters"));
        };
        let mut out = vec![0u8; len];
        out[0] = id;
        out[1] = self.device_index;
        out[2] = self.feature_index;
        out[3] = self.function_byte;
        out[4..4 + self.params.len()].copy_from_slice(&self.params);
        Ok(out)
    }

    pub fn decode(raw: &[u8]) -> Result<Self> {
        let expected = match raw.first() {
            Some(&REPORT_SHORT) => SHORT_LEN,
            Some(&REPORT_LONG) => LONG_LEN,
            Some(_) => return Err(HidppError::Malformed("unknown report id")),
            None => return Err(HidppError::Malformed("empty report")),
        };
        if raw.len() < expected {
            return Err(HidppError::Malformed("truncated report"));
        }
        Ok(Report {
            device_index: raw[1],
            feature_index: raw[2],
            function_byte: raw[3],
            params: raw[4..expected].to_vec(),
        })
    }

    /// Reconhece relatórios de erro.
    ///
    /// HID++ 2.0: `[id, dev, 0xFF, feature_index_que_falhou, função_que_falhou, código]`,
    /// então `function_byte` guarda o índice da feature e `params[1]` o código.
    /// HID++ 1.0: `[id, dev, 0x8F, sub_id, endereço, código, 0]`.
    pub fn as_error(&self) -> Option<HidppError> {
        let code = self.params.get(1).copied().unwrap_or(0);
        match self.feature_index {
            HIDPP2_ERROR_INDEX => Some(HidppError::Protocol {
                feature_index: self.function_byte,
                code,
            }),
            HIDPP1_ERROR_INDEX => Some(HidppError::LegacyProtocol { code }),
            _ => None,
        }
    }
}
```

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p hidpp`
Expected: 9 testes `ok` (8 de `report` + o de fumaça removido; esperar apenas os de `report`), sem warnings de `unused`.

- [ ] **Step 5: Commit**

```bash
git add crates/hidpp
git commit -m "feat(hidpp): add error type and HID++ report codec"
```

---

### Task 3: Transport, FakeTransport e Device (requisição/resposta e descoberta)

**Files:**
- Create: `crates/hidpp/src/transport.rs`
- Create: `crates/hidpp/src/device.rs`
- Modify: `crates/hidpp/src/lib.rs`

**Interfaces:**
- Consumes: `Report`, `HidppError`, `Result` da Task 2.
- Produces:
  - `trait Transport { fn write(&mut self, data: &[u8]) -> Result<()>; fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>>; }` (`None` = tempo esgotado).
  - `FakeTransport` com `new()`, `push_incoming(Vec<u8>)`, campo público `written: Vec<Vec<u8>>`.
  - `pub fn long_reply(device_index: u8, feature_index: u8, function_byte: u8, params: &[u8]) -> Vec<u8>` (relatório longo de 20 bytes, para testes).
  - `Device<T: Transport>`: `new(transport, device_index)`, `with_features(transport, device_index, features: HashMap<u16, u8>)`, `set_timeout(Duration)`, `device_index() -> u8`, `into_transport() -> T`, `request(feature_index: u8, function: u8, params: &[u8]) -> Result<Vec<u8>>`, `ping() -> Result<(u8, u8)>`, `discover() -> Result<()>`, `feature_index(id: u16) -> Result<u8>`, `features() -> Vec<(u16, u8)>` (ordenado por índice), `call(feature_id: u16, function: u8, params: &[u8]) -> Result<Vec<u8>>`.
  - Constantes: `ROOT_FEATURE_INDEX = 0x00`, `FEATURE_SET = 0x0001`, `DEFAULT_TIMEOUT = 2 s`.

- [ ] **Step 1: Escrever `transport.rs` e os testes de `device.rs`**

`crates/hidpp/src/transport.rs`:

```rust
use std::collections::VecDeque;
use std::time::Duration;

use crate::error::Result;

pub trait Transport {
    fn write(&mut self, data: &[u8]) -> Result<()>;
    /// `Ok(None)` quando o tempo esgota sem nada para ler.
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>>;
}

/// Transporte roteirizado para testes: devolve os relatórios enfileirados, na ordem,
/// e `None` (tempo esgotado) quando a fila acaba.
#[derive(Default)]
pub struct FakeTransport {
    pub written: Vec<Vec<u8>>,
    incoming: VecDeque<Vec<u8>>,
}

impl FakeTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_incoming(&mut self, data: Vec<u8>) {
        self.incoming.push_back(data);
    }
}

impl Transport for FakeTransport {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.written.push(data.to_vec());
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>> {
        Ok(self.incoming.pop_front())
    }
}

/// Monta um relatório longo (20 bytes) como um dispositivo o enviaria. Útil em testes.
pub fn long_reply(
    device_index: u8,
    feature_index: u8,
    function_byte: u8,
    params: &[u8],
) -> Vec<u8> {
    let mut v = vec![0x11, device_index, feature_index, function_byte];
    v.extend_from_slice(params);
    v.resize(20, 0);
    v
}
```

`crates/hidpp/src/lib.rs`:

```rust
//! Cliente HID++ 2.0 para dispositivos Logitech.

pub mod device;
pub mod error;
pub mod report;
pub mod transport;

pub use error::{HidppError, Result};
```

`crates/hidpp/src/device.rs` (apenas testes por enquanto):

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::error::{HidppError, Result};
use crate::report::Report;
use crate::transport::Transport;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{long_reply, FakeTransport};

    fn dev(fake: FakeTransport) -> Device<FakeTransport> {
        Device::new(fake, 1)
    }

    #[test]
    fn request_returns_matching_response() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0x00, 0x11, &[4, 2, 0xA5]));
        let mut d = dev(fake);
        let p = d.request(0x00, 1, &[0, 0, 0xA5]).unwrap();
        assert_eq!(&p[..3], &[4, 2, 0xA5]);
    }

    #[test]
    fn request_skips_notifications() {
        let mut fake = FakeTransport::new();
        // Notificação de outra feature (bateria) chega antes da resposta.
        fake.push_incoming(long_reply(1, 0x05, 0x00, &[77, 0, 0]));
        fake.push_incoming(long_reply(1, 0x00, 0x11, &[4, 2, 0xA5]));
        let mut d = dev(fake);
        assert!(d.request(0x00, 1, &[0, 0, 0xA5]).is_ok());
    }

    #[test]
    fn request_times_out_without_response() {
        let mut d = dev(FakeTransport::new());
        assert_eq!(d.request(0x00, 1, &[]), Err(HidppError::Timeout));
    }

    #[test]
    fn request_ignores_malformed_and_foreign_reports() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(vec![0x20, 0, 0]); // ID desconhecido
        fake.push_incoming(vec![0x11, 1]); // truncado
        fake.push_incoming(long_reply(2, 0x00, 0x11, &[9, 9, 9])); // outro índice de dispositivo
        fake.push_incoming(long_reply(1, 0x00, 0x11, &[4, 2, 0xA5]));
        let mut d = dev(fake);
        let p = d.request(0x00, 1, &[0, 0, 0xA5]).unwrap();
        assert_eq!(&p[..3], &[4, 2, 0xA5]);
    }

    #[test]
    fn request_maps_hidpp2_error() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0xFF, 0x04, &[0x21, 0x02]));
        let mut d = dev(fake);
        assert_eq!(
            d.request(0x04, 2, &[]),
            Err(HidppError::Protocol {
                feature_index: 4,
                code: 2
            })
        );
    }

    #[test]
    fn ping_returns_protocol_version() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0x00, 0x11, &[4, 2, 0xA5]));
        let mut d = dev(fake);
        assert_eq!(d.ping(), Ok((4, 2)));
    }

    #[test]
    fn ping_reports_legacy_device() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(vec![0x10, 0x01, 0x8F, 0x00, 0x11, 0x01, 0x00]);
        let mut d = dev(fake);
        assert_eq!(d.ping(), Err(HidppError::LegacyProtocol { code: 1 }));
    }

    #[test]
    fn ping_requires_echo() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0x00, 0x11, &[4, 2, 0x00]));
        let mut d = dev(fake);
        assert_eq!(
            d.ping(),
            Err(HidppError::UnexpectedResponse("ping byte not echoed"))
        );
    }

    #[test]
    fn discover_maps_features_and_rejects_unknown() {
        let mut fake = FakeTransport::new();
        // IRoot.GetFeature(IFeatureSet) -> índice 1
        fake.push_incoming(long_reply(1, 0x00, 0x01, &[1, 0, 0]));
        // IFeatureSet.GetCount -> 2 features além da IRoot
        fake.push_incoming(long_reply(1, 0x01, 0x01, &[2]));
        // GetFeatureID(1) -> 0x1004; GetFeatureID(2) -> 0x2201
        fake.push_incoming(long_reply(1, 0x01, 0x11, &[0x10, 0x04, 0]));
        fake.push_incoming(long_reply(1, 0x01, 0x11, &[0x22, 0x01, 0]));
        let mut d = dev(fake);
        d.discover().unwrap();
        assert_eq!(d.feature_index(0x1004), Ok(1));
        assert_eq!(d.feature_index(0x2201), Ok(2));
        assert_eq!(
            d.feature_index(0x8060),
            Err(HidppError::FeatureNotSupported(0x8060))
        );
        assert_eq!(d.features(), vec![(0x0001, 1), (0x1004, 1), (0x2201, 2)]);
    }

    #[test]
    fn call_on_unsupported_feature_does_not_touch_the_wire() {
        let mut d = dev(FakeTransport::new());
        assert_eq!(
            d.call(0x2201, 0, &[]),
            Err(HidppError::FeatureNotSupported(0x2201))
        );
        assert!(d.into_transport().written.is_empty());
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p hidpp`
Expected: FAIL com erro de compilação (`Device` não definido).

- [ ] **Step 3: Implementar `Device`**

Inserir em `crates/hidpp/src/device.rs`, acima do bloco `#[cfg(test)]`:

```rust
pub const ROOT_FEATURE_INDEX: u8 = 0x00;
pub const FEATURE_SET: u16 = 0x0001;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

const PING_BYTE: u8 = 0xA5;

pub struct Device<T: Transport> {
    transport: T,
    device_index: u8,
    timeout: Duration,
    features: HashMap<u16, u8>,
}

impl<T: Transport> Device<T> {
    pub fn new(transport: T, device_index: u8) -> Self {
        Self::with_features(transport, device_index, HashMap::new())
    }

    /// Cria um dispositivo com o mapa de features já conhecido (útil em testes).
    pub fn with_features(
        transport: T,
        device_index: u8,
        features: HashMap<u16, u8>,
    ) -> Self {
        Device {
            transport,
            device_index,
            timeout: DEFAULT_TIMEOUT,
            features,
        }
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn device_index(&self) -> u8 {
        self.device_index
    }

    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Envia uma requisição e devolve os parâmetros da resposta correspondente.
    ///
    /// Relatórios de outros dispositivos, notificações e lixo são ignorados.
    pub fn request(&mut self, feature_index: u8, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        let req = Report::request(self.device_index, feature_index, function, params);
        self.transport.write(&req.encode()?)?;
        let deadline = Instant::now() + self.timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(HidppError::Timeout);
            }
            let Some(raw) = self.transport.read(remaining)? else {
                return Err(HidppError::Timeout);
            };
            let Ok(resp) = Report::decode(&raw) else {
                continue;
            };
            if resp.device_index != self.device_index {
                continue;
            }
            if let Some(err) = resp.as_error() {
                let ours = match err {
                    HidppError::Protocol { feature_index, .. } => feature_index == req.feature_index,
                    _ => true,
                };
                if ours {
                    return Err(err);
                }
                continue;
            }
            if resp.feature_index == req.feature_index && resp.function_byte == req.function_byte {
                return Ok(resp.params);
            }
        }
    }

    /// IRoot.GetProtocolVersion. Devolve `(maior, menor)`.
    ///
    /// Um dispositivo HID++ 1.0 responde com `LegacyProtocol`.
    pub fn ping(&mut self) -> Result<(u8, u8)> {
        let p = self.request(ROOT_FEATURE_INDEX, 1, &[0, 0, PING_BYTE])?;
        if p[2] != PING_BYTE {
            return Err(HidppError::UnexpectedResponse("ping byte not echoed"));
        }
        Ok((p[0], p[1]))
    }

    /// Descobre o mapa feature-ID → índice via IRoot e IFeatureSet.
    pub fn discover(&mut self) -> Result<()> {
        let p = self.request(ROOT_FEATURE_INDEX, 0, &FEATURE_SET.to_be_bytes())?;
        let set_index = p[0];
        if set_index == 0 {
            return Err(HidppError::FeatureNotSupported(FEATURE_SET));
        }
        let count = self.request(set_index, 0, &[])?[0];
        let mut map = HashMap::new();
        map.insert(FEATURE_SET, set_index);
        for index in 1..=count {
            let p = self.request(set_index, 1, &[index])?;
            map.insert(u16::from_be_bytes([p[0], p[1]]), index);
        }
        self.features = map;
        Ok(())
    }

    pub fn feature_index(&self, id: u16) -> Result<u8> {
        self.features
            .get(&id)
            .copied()
            .ok_or(HidppError::FeatureNotSupported(id))
    }

    /// Features conhecidas como `(id, índice)`, ordenadas por índice.
    pub fn features(&self) -> Vec<(u16, u8)> {
        let mut v: Vec<(u16, u8)> = self.features.iter().map(|(id, idx)| (*id, *idx)).collect();
        v.sort_by_key(|(id, idx)| (*idx, *id));
        v
    }

    /// Chama uma função de uma feature pelo ID dela.
    pub fn call(&mut self, feature_id: u16, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        let index = self.feature_index(feature_id)?;
        self.request(index, function, params)
    }
}
```

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p hidpp`
Expected: todos os testes de `report` e `device` passam (18 no total).

- [ ] **Step 5: Commit**

```bash
git add crates/hidpp
git commit -m "feat(hidpp): add Transport, FakeTransport and Device with feature discovery"
```

---

### Task 4: Features tipadas (bateria, DPI, taxa de polling)

**Files:**
- Create: `crates/hidpp/src/features/mod.rs`
- Create: `crates/hidpp/src/features/battery.rs`
- Create: `crates/hidpp/src/features/dpi.rs`
- Create: `crates/hidpp/src/features/report_rate.rs`
- Modify: `crates/hidpp/src/lib.rs`

**Interfaces:**
- Consumes: `Device<T>::call`, `Device<T>::feature_index`, `Device::with_features`, `FakeTransport`, `long_reply`, `HidppError`.
- Produces:
  - `features::name(id: u16) -> Option<&'static str>`.
  - `battery::{BATTERY_STATUS = 0x1000, UNIFIED_BATTERY = 0x1004, ChargeState {Discharging, Charging, Full, Error, Unknown}, Battery { percent: Option<u8>, state: ChargeState }, read(&mut Device<T>) -> Result<Battery>}`.
  - `dpi::{ADJUSTABLE_DPI = 0x2201, DpiSupport {List(Vec<u16>), Range{min,max,step}}, parse_dpi_list(&[u16]) -> Result<DpiSupport>, supported(&mut Device<T>, sensor: u8) -> Result<DpiSupport>, current(&mut Device<T>, sensor: u8) -> Result<(u16, u16)>, set(&mut Device<T>, sensor: u8, dpi: u16) -> Result<u16>}`.
  - `report_rate::{REPORT_RATE = 0x8060, supported_hz(&mut Device<T>) -> Result<Vec<u16>>, current_hz(&mut Device<T>) -> Result<u16>, set_hz(&mut Device<T>, hz: u16) -> Result<()>}`.

- [ ] **Step 1: Esqueleto do módulo e testes de bateria**

`crates/hidpp/src/lib.rs`:

```rust
//! Cliente HID++ 2.0 para dispositivos Logitech.

pub mod device;
pub mod error;
pub mod features;
pub mod report;
pub mod transport;

pub use error::{HidppError, Result};
```

`crates/hidpp/src/features/mod.rs`:

```rust
pub mod battery;
pub mod dpi;
pub mod report_rate;

/// Nome legível de features HID++ conhecidas (para diagnóstico).
pub fn name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x0000 => "IRoot",
        0x0001 => "IFeatureSet",
        0x0003 => "DeviceFwVersion",
        0x0005 => "DeviceName",
        0x1000 => "BatteryStatus",
        0x1004 => "UnifiedBattery",
        0x1B04 => "ReprogControlsV4",
        0x2201 => "AdjustableDpi",
        0x8060 => "ReportRate",
        0x8071 => "RgbEffects",
        0x8100 => "OnboardProfiles",
        _ => return None,
    })
}
```

`crates/hidpp/src/features/battery.rs` (testes primeiro):

```rust
use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::transport::Transport;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{long_reply, FakeTransport};
    use std::collections::HashMap;

    fn dev(features: &[(u16, u8)], fake: FakeTransport) -> Device<FakeTransport> {
        Device::with_features(fake, 1, features.iter().copied().collect::<HashMap<_, _>>())
    }

    #[test]
    fn reads_unified_battery() {
        let mut fake = FakeTransport::new();
        // feature 0x1004 no índice 1, função 1 (GetStatus): [soc, nível, carga, externa]
        fake.push_incoming(long_reply(1, 1, 0x11, &[80, 0b0010, 0, 0]));
        let mut d = dev(&[(UNIFIED_BATTERY, 1)], fake);
        assert_eq!(
            read(&mut d),
            Ok(Battery {
                percent: Some(80),
                state: ChargeState::Discharging
            })
        );
    }

    #[test]
    fn unified_battery_charging_and_full() {
        for (code, expected) in [(1u8, ChargeState::Charging), (2, ChargeState::Charging), (3, ChargeState::Full)] {
            let mut fake = FakeTransport::new();
            fake.push_incoming(long_reply(1, 1, 0x11, &[50, 0, code, 0]));
            let mut d = dev(&[(UNIFIED_BATTERY, 1)], fake);
            assert_eq!(read(&mut d).unwrap().state, expected);
        }
    }

    #[test]
    fn falls_back_to_battery_status() {
        let mut fake = FakeTransport::new();
        // feature 0x1000 no índice 3, função 0: [nível, próximo nível, status]
        fake.push_incoming(long_reply(1, 3, 0x01, &[55, 40, 1]));
        let mut d = dev(&[(BATTERY_STATUS, 3)], fake);
        assert_eq!(
            read(&mut d),
            Ok(Battery {
                percent: Some(55),
                state: ChargeState::Charging
            })
        );
    }

    #[test]
    fn out_of_range_percent_becomes_none() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 1, 0x11, &[200, 0, 0, 0]));
        let mut d = dev(&[(UNIFIED_BATTERY, 1)], fake);
        assert_eq!(read(&mut d).unwrap().percent, None);
    }

    #[test]
    fn no_battery_feature_is_not_supported() {
        let mut d = dev(&[], FakeTransport::new());
        assert!(matches!(
            read(&mut d),
            Err(HidppError::FeatureNotSupported(_))
        ));
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p hidpp battery`
Expected: FAIL com erro de compilação (`read`, `Battery`… não definidos; os módulos `dpi` e `report_rate` ainda não existem: criar arquivos vazios `dpi.rs` e `report_rate.rs` só para compilar).

- [ ] **Step 3: Implementar a bateria**

Inserir em `battery.rs`, acima do bloco de testes:

```rust
pub const BATTERY_STATUS: u16 = 0x1000;
pub const UNIFIED_BATTERY: u16 = 0x1004;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeState {
    Discharging,
    Charging,
    Full,
    Error,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub percent: Option<u8>,
    pub state: ChargeState,
}

fn percent(raw: u8) -> Option<u8> {
    (raw <= 100).then_some(raw)
}

/// Lê a bateria por UnifiedBattery (0x1004) ou, na falta dela, BatteryStatus (0x1000).
pub fn read<T: Transport>(dev: &mut Device<T>) -> Result<Battery> {
    if dev.feature_index(UNIFIED_BATTERY).is_ok() {
        let p = dev.call(UNIFIED_BATTERY, 1, &[])?;
        let state = match p[2] {
            0 => ChargeState::Discharging,
            1 | 2 => ChargeState::Charging,
            3 => ChargeState::Full,
            4 => ChargeState::Error,
            _ => ChargeState::Unknown,
        };
        return Ok(Battery {
            percent: percent(p[0]),
            state,
        });
    }
    let p = dev.call(BATTERY_STATUS, 0, &[]).map_err(|e| match e {
        HidppError::FeatureNotSupported(_) => HidppError::FeatureNotSupported(BATTERY_STATUS),
        other => other,
    })?;
    let state = match p[2] {
        0 => ChargeState::Discharging,
        1 | 2 | 4 => ChargeState::Charging,
        3 => ChargeState::Full,
        5..=7 => ChargeState::Error,
        _ => ChargeState::Unknown,
    };
    Ok(Battery {
        percent: percent(p[0]),
        state,
    })
}
```

- [ ] **Step 4: Testes e implementação de DPI**

`crates/hidpp/src/features/dpi.rs`:

```rust
use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::transport::Transport;

pub const ADJUSTABLE_DPI: u16 = 0x2201;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DpiSupport {
    List(Vec<u16>),
    Range { min: u16, max: u16, step: u16 },
}

/// Interpreta a lista de DPI do dispositivo: `[min, 0xE000|passo, max]` é uma faixa;
/// caso contrário são valores explícitos. Zero encerra a lista.
pub fn parse_dpi_list(words: &[u16]) -> Result<DpiSupport> {
    let words: Vec<u16> = words.iter().copied().take_while(|w| *w != 0).collect();
    if words.len() >= 3 && words[1] & 0xE000 == 0xE000 {
        return Ok(DpiSupport::Range {
            min: words[0],
            step: words[1] & 0x1FFF,
            max: words[2],
        });
    }
    if words.is_empty() {
        return Err(HidppError::UnexpectedResponse("empty DPI list"));
    }
    Ok(DpiSupport::List(words))
}

pub fn supported<T: Transport>(dev: &mut Device<T>, sensor: u8) -> Result<DpiSupport> {
    let p = dev.call(ADJUSTABLE_DPI, 1, &[sensor])?;
    let words: Vec<u16> = p[1..]
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    parse_dpi_list(&words)
}

/// Devolve `(dpi_atual, dpi_padrão)`.
pub fn current<T: Transport>(dev: &mut Device<T>, sensor: u8) -> Result<(u16, u16)> {
    let p = dev.call(ADJUSTABLE_DPI, 2, &[sensor])?;
    Ok((
        u16::from_be_bytes([p[1], p[2]]),
        u16::from_be_bytes([p[3], p[4]]),
    ))
}

/// Define o DPI e devolve o valor que o dispositivo aplicou.
pub fn set<T: Transport>(dev: &mut Device<T>, sensor: u8, dpi: u16) -> Result<u16> {
    let [hi, lo] = dpi.to_be_bytes();
    let p = dev.call(ADJUSTABLE_DPI, 3, &[sensor, hi, lo])?;
    Ok(u16::from_be_bytes([p[1], p[2]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{long_reply, FakeTransport};
    use std::collections::HashMap;

    fn dev(fake: FakeTransport) -> Device<FakeTransport> {
        Device::with_features(fake, 1, HashMap::from([(ADJUSTABLE_DPI, 2u8)]))
    }

    #[test]
    fn parses_range() {
        assert_eq!(
            parse_dpi_list(&[200, 0xE000 | 50, 25600, 0]),
            Ok(DpiSupport::Range {
                min: 200,
                step: 50,
                max: 25600
            })
        );
    }

    #[test]
    fn parses_explicit_list_and_stops_at_zero() {
        assert_eq!(
            parse_dpi_list(&[400, 800, 1600, 0, 0]),
            Ok(DpiSupport::List(vec![400, 800, 1600]))
        );
    }

    #[test]
    fn rejects_empty_list() {
        assert_eq!(
            parse_dpi_list(&[0, 0]),
            Err(HidppError::UnexpectedResponse("empty DPI list"))
        );
    }

    #[test]
    fn reads_supported_range_from_device() {
        let mut fake = FakeTransport::new();
        // função 1 -> fbyte 0x11; params: [sensor, min, 0xE000|passo, max]
        fake.push_incoming(long_reply(1, 2, 0x11, &[0, 0x00, 0xC8, 0xE0, 0x32, 0x64, 0x00]));
        let mut d = dev(fake);
        assert_eq!(
            supported(&mut d, 0),
            Ok(DpiSupport::Range {
                min: 200,
                step: 50,
                max: 25600
            })
        );
    }

    #[test]
    fn reads_current_and_default() {
        let mut fake = FakeTransport::new();
        // função 2 -> fbyte 0x21; params: [sensor, atual(2), padrão(2)]
        fake.push_incoming(long_reply(1, 2, 0x21, &[0, 0x06, 0x40, 0x03, 0x20]));
        let mut d = dev(fake);
        assert_eq!(current(&mut d, 0), Ok((1600, 800)));
    }

    #[test]
    fn sets_dpi_and_returns_applied_value() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 2, 0x31, &[0, 0x0C, 0x80]));
        let mut d = dev(fake);
        assert_eq!(set(&mut d, 0, 3200), Ok(3200));
        let t = d.into_transport();
        assert_eq!(t.written[0], vec![0x10, 1, 2, 0x31, 0, 0x0C, 0x80]);
    }

    #[test]
    fn unsupported_dpi_feature() {
        let mut d = Device::with_features(FakeTransport::new(), 1, HashMap::new());
        assert_eq!(
            current(&mut d, 0),
            Err(HidppError::FeatureNotSupported(ADJUSTABLE_DPI))
        );
    }
}
```

- [ ] **Step 5: Testes e implementação da taxa de polling**

`crates/hidpp/src/features/report_rate.rs`:

```rust
use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::transport::Transport;

pub const REPORT_RATE: u16 = 0x8060;

/// Taxas suportadas em Hz. O bit `i` da máscara indica intervalo de `i + 1` ms.
pub fn supported_hz<T: Transport>(dev: &mut Device<T>) -> Result<Vec<u16>> {
    let p = dev.call(REPORT_RATE, 0, &[])?;
    Ok((0..8u16)
        .filter(|i| p[0] & (1 << i) != 0)
        .map(|i| 1000 / (i + 1))
        .collect())
}

pub fn current_hz<T: Transport>(dev: &mut Device<T>) -> Result<u16> {
    let p = dev.call(REPORT_RATE, 1, &[])?;
    if p[0] == 0 {
        return Err(HidppError::UnexpectedResponse("report interval is zero"));
    }
    Ok(1000 / u16::from(p[0]))
}

/// Só aceita taxas que correspondem a um intervalo inteiro de 1 a 8 ms.
pub fn set_hz<T: Transport>(dev: &mut Device<T>, hz: u16) -> Result<()> {
    if hz == 0 || 1000 % hz != 0 || !(1..=8).contains(&(1000 / hz)) {
        return Err(HidppError::InvalidArgument("unsupported polling rate"));
    }
    dev.call(REPORT_RATE, 2, &[(1000 / hz) as u8])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{long_reply, FakeTransport};
    use std::collections::HashMap;

    fn dev(fake: FakeTransport) -> Device<FakeTransport> {
        Device::with_features(fake, 1, HashMap::from([(REPORT_RATE, 4u8)]))
    }

    #[test]
    fn lists_supported_rates() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 4, 0x01, &[0b1000_1011]));
        let mut d = dev(fake);
        assert_eq!(supported_hz(&mut d), Ok(vec![1000, 500, 250, 125]));
    }

    #[test]
    fn reads_current_rate() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 4, 0x11, &[1]));
        let mut d = dev(fake);
        assert_eq!(current_hz(&mut d), Ok(1000));
    }

    #[test]
    fn zero_interval_is_unexpected() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 4, 0x11, &[0]));
        let mut d = dev(fake);
        assert_eq!(
            current_hz(&mut d),
            Err(HidppError::UnexpectedResponse("report interval is zero"))
        );
    }

    #[test]
    fn sets_rate_as_interval() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 4, 0x21, &[2]));
        let mut d = dev(fake);
        set_hz(&mut d, 500).unwrap();
        let t = d.into_transport();
        assert_eq!(t.written[0], vec![0x10, 1, 4, 0x21, 2, 0, 0]);
    }

    #[test]
    fn invalid_rates_never_reach_the_wire() {
        for hz in [0u16, 333, 2000, 100] {
            let mut d = dev(FakeTransport::new());
            assert_eq!(
                set_hz(&mut d, hz),
                Err(HidppError::InvalidArgument("unsupported polling rate")),
                "hz = {hz}"
            );
            assert!(d.into_transport().written.is_empty());
        }
    }
}
```

- [ ] **Step 6: Rodar tudo**

Run: `cargo test -p hidpp && cargo clippy -p hidpp --all-targets -- -D warnings`
Expected: todos os testes passam; clippy sem avisos. Se o clippy apontar problemas de estilo nos testes (ex.: `unwrap` ou conversão), corrigir sem mudar o comportamento.

- [ ] **Step 7: Commit**

```bash
git add crates/hidpp
git commit -m "feat(hidpp): add typed battery, DPI and polling-rate features"
```

---

### Task 5: Transporte hidraw real e enumeração

**Files:**
- Create: `crates/hidpp/src/hidraw.rs`
- Create: `crates/hidpp/src/enumerate.rs`
- Modify: `crates/hidpp/src/lib.rs`

**Interfaces:**
- Consumes: `Transport`, `HidppError`.
- Produces:
  - `HidrawTransport::open(path: &Path) -> Result<HidrawTransport>` (implementa `Transport`).
  - `enumerate::HidrawInfo { path: PathBuf, vendor_id: u16, product_id: u16, name: String }`.
  - `enumerate::parse_uevent(text: &str) -> Option<(u16, u16, String)>`.
  - `enumerate::has_hidpp_collection(descriptor: &[u8]) -> bool`.
  - `enumerate::list_logitech() -> Result<Vec<HidrawInfo>>` (ordenada por caminho).

- [ ] **Step 1: Testes de `enumerate.rs`**

`crates/hidpp/src/enumerate.rs`:

```rust
use std::fs;
use std::path::PathBuf;

use crate::error::Result;

#[cfg(test)]
mod tests {
    use super::*;

    const UEVENT: &str = "DRIVER=hid-generic\n\
HID_ID=0003:0000046D:0000C539\n\
HID_NAME=Logitech USB Receiver\n\
HID_PHYS=usb-0000:00:14.0-2/input2\n";

    #[test]
    fn parses_uevent() {
        assert_eq!(
            parse_uevent(UEVENT),
            Some((0x046d, 0xc539, "Logitech USB Receiver".to_string()))
        );
    }

    #[test]
    fn uevent_without_hid_id_is_none() {
        assert_eq!(parse_uevent("DRIVER=hid-generic\n"), None);
    }

    #[test]
    fn uevent_with_garbage_id_is_none() {
        assert_eq!(parse_uevent("HID_ID=zzzz\n"), None);
    }

    #[test]
    fn detects_hidpp_descriptor() {
        // Usage Page 0xFF00, Report ID 0x11
        let d = [
            0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x11, 0x75, 0x08, 0x95, 0x13, 0xC0,
        ];
        assert!(has_hidpp_collection(&d));
    }

    #[test]
    fn rejects_keyboard_descriptor() {
        let d = [0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, 0x05, 0x07, 0xC0];
        assert!(!has_hidpp_collection(&d));
    }

    #[test]
    fn vendor_page_without_report_id_11_is_rejected() {
        let d = [0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x10, 0xC0];
        assert!(!has_hidpp_collection(&d));
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p hidpp enumerate`
Expected: FAIL (funções não definidas). Antes, adicionar `pub mod enumerate; pub mod hidraw;` em `lib.rs` e criar `hidraw.rs` vazio para compilar.

- [ ] **Step 3: Implementar a enumeração**

Inserir em `enumerate.rs`, acima dos testes:

```rust
const LOGITECH_VENDOR: u16 = 0x046d;
const SYSFS_HIDRAW: &str = "/sys/class/hidraw";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidrawInfo {
    pub path: PathBuf,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
}

/// Lê `HID_ID=bus:vendor:produto` e `HID_NAME=` do uevent de um dispositivo HID.
pub fn parse_uevent(text: &str) -> Option<(u16, u16, String)> {
    let mut ids = None;
    let mut name = String::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("HID_ID=") {
            let mut parts = v.split(':');
            let _bus = parts.next()?;
            let vendor = u32::from_str_radix(parts.next()?, 16).ok()?;
            let product = u32::from_str_radix(parts.next()?, 16).ok()?;
            ids = Some((u16::try_from(vendor).ok()?, u16::try_from(product).ok()?));
        } else if let Some(v) = line.strip_prefix("HID_NAME=") {
            name = v.to_string();
        }
    }
    ids.map(|(v, p)| (v, p, name))
}

/// A interface HID++ expõe uma página de uso do fabricante (0xFF00 ou 0xFF43)
/// e o Report ID 0x11 (relatório longo).
pub fn has_hidpp_collection(descriptor: &[u8]) -> bool {
    let has = |needle: &[u8]| descriptor.windows(needle.len()).any(|w| w == needle);
    (has(&[0x06, 0x00, 0xFF]) || has(&[0x06, 0x43, 0xFF])) && has(&[0x85, 0x11])
}

/// Lista as interfaces hidraw da Logitech que falam HID++, ordenadas por caminho.
pub fn list_logitech() -> Result<Vec<HidrawInfo>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(SYSFS_HIDRAW)? {
        let entry = entry?;
        let node = entry.file_name().to_string_lossy().into_owned();
        let device_dir = entry.path().join("device");
        let Ok(uevent) = fs::read_to_string(device_dir.join("uevent")) else {
            continue;
        };
        let Some((vendor_id, product_id, name)) = parse_uevent(&uevent) else {
            continue;
        };
        if vendor_id != LOGITECH_VENDOR {
            continue;
        }
        let Ok(descriptor) = fs::read(device_dir.join("report_descriptor")) else {
            continue;
        };
        if !has_hidpp_collection(&descriptor) {
            continue;
        }
        out.push(HidrawInfo {
            path: PathBuf::from("/dev").join(node),
            vendor_id,
            product_id,
            name,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}
```

- [ ] **Step 4: Implementar `HidrawTransport`**

`crates/hidpp/src/hidraw.rs`:

```rust
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::Duration;

use crate::error::Result;
use crate::transport::Transport;

/// Transporte sobre `/dev/hidrawN`. Os relatórios incluem o Report ID no primeiro byte.
pub struct HidrawTransport {
    file: File,
}

impl HidrawTransport {
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(HidrawTransport { file })
    }
}

impl Transport for HidrawTransport {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.file.write_all(data)?;
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let mut pfd = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
        // SAFETY: `pfd` é uma estrutura válida e vive durante toda a chamada; passamos 1 elemento.
        let rc = unsafe { libc::poll(&mut pfd, 1, ms) };
        if rc < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(err.into());
        }
        if rc == 0 {
            return Ok(None);
        }
        let mut buf = [0u8; 64];
        let n = self.file.read(&mut buf)?;
        Ok(Some(buf[..n].to_vec()))
    }
}
```

`crates/hidpp/src/lib.rs`:

```rust
//! Cliente HID++ 2.0 para dispositivos Logitech.

pub mod device;
pub mod enumerate;
pub mod error;
pub mod features;
pub mod hidraw;
pub mod report;
pub mod transport;

pub use error::{HidppError, Result};
```

- [ ] **Step 5: Rodar tudo**

Run: `cargo test -p hidpp && cargo clippy -p hidpp --all-targets -- -D warnings`
Expected: PASS e sem avisos.

- [ ] **Step 6: Commit**

```bash
git add crates/hidpp
git commit -m "feat(hidpp): add hidraw transport and Logitech interface enumeration"
```

---

### Task 6: CLI `ghubctl list` e `ghubctl probe`

**Files:**
- Modify: `crates/ghubctl/src/main.rs`

**Interfaces:**
- Consumes: `hidpp::enumerate::list_logitech`, `hidpp::hidraw::HidrawTransport`, `hidpp::device::{Device, DEFAULT_TIMEOUT}`, `hidpp::features::{name, battery::read, dpi::{current, supported}, report_rate::{current_hz, supported_hz}}`.
- Produces: binário `ghubctl` com os subcomandos `list` e `probe`. O `probe` é somente leitura.

- [ ] **Step 1: Implementar**

`crates/ghubctl/src/main.rs`:

```rust
use std::error::Error;
use std::time::Duration;

use clap::{Parser, Subcommand};
use hidpp::device::{Device, DEFAULT_TIMEOUT};
use hidpp::enumerate::list_logitech;
use hidpp::features::{self, battery, dpi, report_rate};
use hidpp::hidraw::HidrawTransport;
use hidpp::transport::Transport;

#[derive(Parser)]
#[command(name = "ghubctl", about = "Diagnóstico e controle de dispositivos Logitech")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lista as interfaces hidraw HID++ da Logitech
    List,
    /// Descobre dispositivos e features (somente leitura)
    Probe,
}

/// Índices tentados em cada interface: slots do receptor (1-6), o próprio receptor (0xFF)
/// e um dispositivo conectado direto (0x00).
const CANDIDATE_INDICES: [u8; 8] = [1, 2, 3, 4, 5, 6, 0xFF, 0x00];
const PING_TIMEOUT: Duration = Duration::from_millis(400);

fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::List => list(),
        Command::Probe => probe(),
    }
}

fn list() -> Result<(), Box<dyn Error>> {
    let interfaces = list_logitech()?;
    if interfaces.is_empty() {
        println!("Nenhuma interface HID++ da Logitech encontrada.");
    }
    for i in interfaces {
        println!(
            "{}  {:04x}:{:04x}  {}",
            i.path.display(),
            i.vendor_id,
            i.product_id,
            i.name
        );
    }
    Ok(())
}

fn probe() -> Result<(), Box<dyn Error>> {
    let interfaces = list_logitech()?;
    if interfaces.is_empty() {
        println!("Nenhuma interface HID++ da Logitech encontrada.");
        return Ok(());
    }
    for info in interfaces {
        println!(
            "{}  {:04x}:{:04x}  {}",
            info.path.display(),
            info.vendor_id,
            info.product_id,
            info.name
        );
        let mut transport = match HidrawTransport::open(&info.path) {
            Ok(t) => t,
            Err(e) => {
                println!("  não foi possível abrir: {e}");
                println!("  (sem permissão? veja docs/udev/70-ghub-linux.rules)");
                continue;
            }
        };
        for index in CANDIDATE_INDICES {
            let mut dev = Device::new(transport, index);
            dev.set_timeout(PING_TIMEOUT);
            match dev.ping() {
                Ok((major, minor)) => {
                    println!("  índice 0x{index:02x}: HID++ {major}.{minor}");
                    dev.set_timeout(DEFAULT_TIMEOUT);
                    match dev.discover() {
                        Ok(()) => describe(&mut dev),
                        Err(e) => println!("    falha ao listar features: {e}"),
                    }
                }
                Err(e) => println!("  índice 0x{index:02x}: {e}"),
            }
            transport = dev.into_transport();
        }
    }
    Ok(())
}

/// Imprime features e valores atuais. Só leituras; nunca escreve no dispositivo.
fn describe<T: Transport>(dev: &mut Device<T>) {
    println!("    features:");
    for (id, index) in dev.features() {
        println!(
            "      0x{id:04x} (índice {index}) {}",
            features::name(id).unwrap_or("?")
        );
    }
    match battery::read(dev) {
        Ok(b) => println!("    bateria: {b:?}"),
        Err(e) => println!("    bateria: {e}"),
    }
    match dpi::current(dev, 0) {
        Ok((now, default)) => println!("    DPI atual: {now} (padrão {default})"),
        Err(e) => println!("    DPI atual: {e}"),
    }
    match dpi::supported(dev, 0) {
        Ok(s) => println!("    DPI suportado: {s:?}"),
        Err(e) => println!("    DPI suportado: {e}"),
    }
    match report_rate::current_hz(dev) {
        Ok(hz) => println!("    polling atual: {hz} Hz"),
        Err(e) => println!("    polling atual: {e}"),
    }
    match report_rate::supported_hz(dev) {
        Ok(v) => println!("    polling suportado: {v:?} Hz"),
        Err(e) => println!("    polling suportado: {e}"),
    }
}
```

- [ ] **Step 2: Compilar e checar estilo**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: compila sem avisos.

- [ ] **Step 3: Rodar `list` (sem abrir dispositivos)**

Run: `cargo run -q -p ghubctl -- list`
Expected: com o receptor conectado, uma linha como `/dev/hidrawN  046d:c539  Logitech USB Receiver`. Se vier "Nenhuma interface", conferir `ls /sys/class/hidraw` e reportar, sem alterar o código.

- [ ] **Step 4: Commit**

```bash
git add crates/ghubctl
git commit -m "feat(ghubctl): add list and read-only probe commands"
```

---

### Task 7: Probe no hardware real e registro dos resultados

**Files:**
- Create: `docs/udev/70-ghub-linux.rules`
- Create: `docs/hardware/g703-probe.md`
- Modify: `docs/superpowers/specs/2026-10-12-ghub-linux-design.md` (seção 11, item "IDs reais das features")

**Interfaces:**
- Consumes: binário `ghubctl` da Task 6.
- Produces: o registro das features reais do G703. Os Planos 2 a 4 se baseiam nele.

- [ ] **Step 1: Criar a regra udev (arquivo no repositório, ainda não instalado)**

`docs/udev/70-ghub-linux.rules`:

```
# Concede ao usuário logado acesso aos nós hidraw de dispositivos Logitech (HID++).
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="046d", TAG+="uaccess"
```

- [ ] **Step 2: Tentar o probe sem a regra**

Run: `cargo run -q -p ghubctl -- probe`
Expected: ou as features aparecem (já há permissão), ou `não foi possível abrir: ... Permission denied`.

- [ ] **Step 3: Se faltar permissão, o USUÁRIO instala a regra**

**Não executar `sudo` sem autorização explícita.** Pedir ao usuário para rodar, com o prefixo `!` no prompt:

```bash
! sudo cp docs/udev/70-ghub-linux.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=hidraw
```

Depois, o usuário pode precisar reconectar o receptor. Repetir o Step 2.

- [ ] **Step 4: Rodar o probe com o G703 ligado e salvar a saída**

Mover o mouse antes de rodar, para ele não estar dormindo.

Run: `mkdir -p docs/hardware && cargo run -q -p ghubctl -- probe 2>&1 | tee /tmp/g703-probe.txt`
Expected: pelo menos um índice responde `HID++ 4.2` (ou `2.0`), com a lista de features. Se nenhum índice responder, **não adivinhar**: registrar a saída exata e parar para discutir com o usuário (pode ser mouse dormindo, índice diferente ou HID++ 1.0).

- [ ] **Step 5: Registrar os resultados**

Criar `docs/hardware/g703-probe.md` com: data, saída completa do probe (bloco de código), e uma tabela "hipótese vs. observado" para cada feature do plano (`0x1000`, `0x1004`, `0x2201`, `0x8060`, `0x8071`, `0x8100`, `0x1B04`): presente ou ausente, e o índice. Anotar também o índice de dispositivo do mouse no receptor e qualquer valor que contradiga as hipóteses (por exemplo bateria por `0x1000` em vez de `0x1004`, ou o DPI máximo real).

- [ ] **Step 6: Atualizar a spec**

Em `docs/superpowers/specs/2026-10-12-ghub-linux-design.md`, seção 11, trocar o item "IDs reais das features do G703 (resolvido pelo spike do passo 1)." por "IDs reais das features do G703: resolvido, ver `docs/hardware/g703-probe.md`." e ajustar a seção 5.1 se alguma feature confirmada for diferente da hipótese.

- [ ] **Step 7: Commit**

```bash
git add docs
git commit -m "docs: record G703 hardware probe and add udev rule"
```

- [ ] **Step 8: Verificação final do plano**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: tudo passa. O entregável é: `hidpp` testada, `ghubctl probe` funcionando e os resultados reais do G703 registrados.

---

## Auto-revisão (feita ao escrever)

- **Cobertura da spec:** seção 5.1 (Transport, FakeTransport, enumeração, descoberta de features, bateria/DPI/polling, erros tipados) → Tasks 2 a 5. Seção 10, passos 1 e 2 → Tasks 6 e 7. Regra de ouro ("só usa o que o dispositivo anuncia") → `FeatureNotSupported` em `Device::feature_index`/`call`. Seção 7 (udev, sem `sudo` automático) → Task 7, Steps 1 e 3. `ghubctl install-udev` fica para um plano posterior.
- **Fora deste plano, por dependerem do resultado da Task 7:** `ghub-ipc`, `ghubd`, bandeja, autostart, GUI (Planos 2 a 4).
- **Consistência de tipos:** `Device::request/call/ping/discover/feature_index/features/with_features/into_transport`, `long_reply`, `Battery/ChargeState`, `DpiSupport`, `HidppError` usam os mesmos nomes e assinaturas em todas as tasks.
- **Sem placeholders:** todos os passos de código trazem o código completo. A única parte aberta é o conteúdo do `g703-probe.md`, que só existe depois de rodar no hardware (Step 5 diz exatamente o que registrar).
- **Nota sobre o Step 4 da Task 2:** a contagem de testes esperada ali é a de `report` (8 testes); o teste de fumaça da Task 1 é removido ao reescrever `lib.rs`.
