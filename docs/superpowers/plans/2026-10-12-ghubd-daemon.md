# ghubd Daemon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Entregar o serviço de usuário `ghubd`: descobre o G703 pelo receptor, aplica e mantém um perfil (DPI e polling), expõe tudo por D-Bus e reaplica o perfil quando o mouse reconecta.

**Architecture:** Uma única thread "gerente" é dona dos dispositivos e do perfil; recebe comandos por canal (`mpsc`) vindos do serviço D-Bus e do monitor udev, e faz um ciclo periódico (ping, rescan, bateria). Um trait `Backend` (descoberta real via sysfs/hidraw, ou falsa nos testes) e um trait `Mouse` (HID++ real ou falso) isolam o hardware. Perfis são TOML com escrita atômica. A bateria vem do sysfs do kernel. Sem tokio: threads e canais; `zbus` bloqueante.

**Tech Stack:** Rust 2021; `zbus` 5 (bloqueante), `zvariant` 5, `serde`, `toml` 1, `udev` 0.9, `libc`, `tempfile` (testes). Dependências já validadas num protótipo descartável (structs D-Bus, proxy bloqueante, sinais com argumento, monitor udev, ida e volta TOML).

**Spec:** `docs/superpowers/specs/2026-10-12-ghub-linux-design.md` (seções 5.2, 5.3, 8, 9). Achados do hardware em `docs/hardware/g703-probe.md`. Este é o **Plano 2 de 4**; a bandeja (Plano 3) e a GUI (Plano 4) consomem a API D-Bus definida aqui.

## Global Constraints

- Rust edição 2021; `RUSTFLAGS="-D warnings" cargo test --workspace` é o critério de qualidade (clippy e rustfmt não estão instalados; não instalar).
- Nenhum `sudo` é executado pelo plano. Escritas em hardware real só na Task 9, com autorização explícita do usuário.
- Barramento: sessão. Nome `org.ghublinux.Daemon`, caminho `/org/ghublinux/Daemon`, interface `org.ghublinux.Daemon1`.
- O daemon nunca escreve no dispositivo um valor que o dispositivo não anunciou como suportado (DPI contra `DpiSupport`, polling contra `supported_hz`).
- A primeira execução (sem `profiles.toml`) **não altera** o mouse: o perfil padrão é semeado a partir dos valores atuais do dispositivo.
- `profiles.toml` é escrito de forma atômica (arquivo temporário + `rename`); um arquivo inválido vira `profiles.toml.bak` e o daemon sobe com o padrão.
- Testes unitários não dependem de hardware, de `/dev/hidraw*` nem do barramento D-Bus de sessão.
- Um perfil vale para todos os dispositivos conectados nesta versão; os níveis de DPI do perfil são predefinições no host (não são gravados na memória onboard do mouse).
- Bateria só por sysfs (`/sys/class/power_supply/hidpp_battery_*`), associada ao dispositivo apenas quando o receptor tem exatamente um dispositivo e uma bateria.
- IDs de dispositivo: `{vendor:04x}{produto:04x}-{slot}` do receptor, por exemplo `046dc539-1`.

## Review Focus

Condições que a spec implica, mas que testes óbvios não cobririam, da mais provável para a menos:

1. Mouse dormindo ou desligado no meio de uma operação: comandos devolvem erro (`Offline`/`Device`), não travam; o ciclo marca offline; ao voltar, o perfil é reaplicado (Task 6).
2. Primeira execução não pode mudar DPI nem polling do mouse (semear a partir do dispositivo e não escrever valores iguais aos atuais) (Task 6).
3. `profiles.toml` corrompido, vazio ou com perfil inválido: daemon sobe com o padrão, guarda `.bak`, nunca entra em pânico (Task 3).
4. Entrada de clientes D-Bus não confiáveis: `u32` fora de `u16`, 0 ou mais de 5 níveis, `active` fora da lista, DPI ou polling não suportado, nome de perfil vazio, enorme ou com caracteres de controle → `InvalidArgs` sem escrever no dispositivo (Tasks 3, 6 e 7).
5. Gerente ocupado ou parado: o serviço D-Bus responde `Failed` por tempo esgotado em vez de travar o cliente (Task 7).

---

### Task 1: Endurecimento da `hidpp` e leitura do nome do dispositivo

**Files:**
- Modify: `crates/hidpp/src/transport.rs` (tornar `fb` público)
- Modify: `crates/hidpp/src/hidraw.rs`
- Modify: `crates/hidpp/src/device.rs` (`discover` ignora feature id 0)
- Create: `crates/hidpp/src/features/device_name.rs`
- Modify: `crates/hidpp/src/features/mod.rs`

**Interfaces:**
- Consumes: `Device::call`, `FakeTransport`, `long_reply`, `HidppError`.
- Produces:
  - `hidpp::transport::fb(function: u8) -> u8` (pública).
  - `HidrawTransport::from_file(file: File) -> HidrawTransport`; `read` repete o `poll` em `EINTR` com o tempo restante e devolve `HidppError::Io` em `POLLERR`/`POLLHUP`/`POLLNVAL` ou leitura de 0 bytes.
  - `hidpp::features::device_name::{DEVICE_NAME = 0x0005, read(&mut Device<T>) -> Result<String>}`.

- [ ] **Step 1: Tornar `fb` público**

Em `crates/hidpp/src/transport.rs`, substituir:

```rust
/// Byte de função como o enviamos: função no nibble alto, nosso `SW_ID` no baixo.
#[cfg(test)]
pub(crate) fn fb(function: u8) -> u8 {
    (function << 4) | crate::report::SW_ID
}
```

por:

```rust
/// Byte de função como o enviamos: função no nibble alto, nosso `SW_ID` no baixo.
/// Público para que os testes de outros crates montem respostas do dispositivo.
pub fn fb(function: u8) -> u8 {
    (function << 4) | crate::report::SW_ID
}
```

- [ ] **Step 2: Escrever os testes que falham (hidraw e discover)**

Substituir `crates/hidpp/src/hidraw.rs` por (a implementação ainda é a antiga no `read`; os testes novos falham):

```rust
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::error::{HidppError, Result};
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

    /// Envolve um arquivo já aberto (usado nos testes com pipes).
    pub fn from_file(file: File) -> Self {
        HidrawTransport { file }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::pipe;
    use std::os::fd::OwnedFd;

    fn pipe_transport() -> (HidrawTransport, std::io::PipeWriter) {
        let (reader, writer) = pipe().unwrap();
        (
            HidrawTransport::from_file(File::from(OwnedFd::from(reader))),
            writer,
        )
    }

    #[test]
    fn read_times_out_when_nothing_arrives() {
        let (mut t, _writer) = pipe_transport();
        let start = Instant::now();
        assert_eq!(t.read(Duration::from_millis(50)), Ok(None));
        assert!(start.elapsed() >= Duration::from_millis(40));
    }

    #[test]
    fn read_returns_available_bytes() {
        let (mut t, mut writer) = pipe_transport();
        writer.write_all(&[0x11, 1, 2, 3]).unwrap();
        assert_eq!(
            t.read(Duration::from_millis(200)),
            Ok(Some(vec![0x11, 1, 2, 3]))
        );
    }

    #[test]
    fn read_reports_hangup_as_io_error() {
        let (mut t, writer) = pipe_transport();
        drop(writer);
        assert!(matches!(
            t.read(Duration::from_millis(200)),
            Err(HidppError::Io(_))
        ));
    }
}
```

Em `crates/hidpp/src/device.rs`, dentro do `mod tests`, acrescentar:

```rust
    #[test]
    fn discover_ignores_feature_id_zero() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0x00, fb(0), &[1, 0, 0]));
        fake.push_incoming(long_reply(1, 0x01, fb(0), &[2]));
        fake.push_incoming(long_reply(1, 0x01, fb(1), &[0x00, 0x00, 0]));
        fake.push_incoming(long_reply(1, 0x01, fb(1), &[0x22, 0x01, 0]));
        let mut d = dev(fake);
        d.discover().unwrap();
        assert_eq!(d.features(), vec![(0x0001, 1), (0x2201, 2)]);
    }
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p hidpp`
Expected: exatamente dois testes FALHAM: `hidraw::tests::read_reports_hangup_as_io_error` (hoje devolve `Ok(Some(vec![]))`) e `device::tests::discover_ignores_feature_id_zero` (o mapa traz a feature `0x0000`). Os demais passam (inclusive os outros dois testes novos de `hidraw`).

- [ ] **Step 4: Implementar**

Em `hidraw.rs`, substituir o corpo de `fn read` por:

```rust
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut pfd = libc::pollfd {
                fd: self.file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ms = remaining.as_millis().min(i32::MAX as u128) as libc::c_int;
            // SAFETY: `pfd` é uma estrutura válida e vive durante toda a chamada; passamos 1 elemento.
            let rc = unsafe { libc::poll(&mut pfd, 1, ms) };
            if rc < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err.into());
            }
            if rc == 0 {
                return Ok(None);
            }
            if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(HidppError::Io(
                    "hidraw device error or disconnected".to_string(),
                ));
            }
            let mut buf = [0u8; 64];
            let n = self.file.read(&mut buf)?;
            if n == 0 {
                return Err(HidppError::Io("hidraw returned an empty read".to_string()));
            }
            return Ok(Some(buf[..n].to_vec()));
        }
    }
```

Em `device.rs`, no `discover`, substituir o corpo do laço por:

```rust
        for index in 1..=count {
            let p = self.request(set_index, 1, &[index])?;
            let id = u16::from_be_bytes([p[0], p[1]]);
            if id == 0 {
                continue;
            }
            map.insert(id, index);
        }
```

- [ ] **Step 5: Testes e implementação do nome do dispositivo**

`crates/hidpp/src/features/device_name.rs`:

```rust
use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::transport::Transport;

pub const DEVICE_NAME: u16 = 0x0005;

/// Lê o nome do dispositivo (função 0: tamanho; função 1: bytes a partir de um deslocamento).
pub fn read<T: Transport>(dev: &mut Device<T>) -> Result<String> {
    let len = usize::from(dev.call(DEVICE_NAME, 0, &[])?[0]);
    let mut out: Vec<u8> = Vec::with_capacity(len);
    while out.len() < len {
        let p = dev.call(DEVICE_NAME, 1, &[out.len() as u8])?;
        let take = (len - out.len()).min(p.len());
        let chunk = &p[..take];
        let nul = chunk.iter().position(|b| *b == 0).unwrap_or(chunk.len());
        if nul == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..nul]);
        if nul < chunk.len() {
            break;
        }
    }
    String::from_utf8(out)
        .map_err(|_| HidppError::UnexpectedResponse("device name is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{fb, long_reply, FakeTransport};
    use std::collections::HashMap;

    fn dev(fake: FakeTransport) -> Device<FakeTransport> {
        Device::with_features(fake, 1, HashMap::from([(DEVICE_NAME, 3u8)]))
    }

    #[test]
    fn reads_short_name() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 3, fb(0), &[7]));
        fake.push_incoming(long_reply(1, 3, fb(1), b"G703 LS"));
        assert_eq!(read(&mut dev(fake)), Ok("G703 LS".to_string()));
    }

    #[test]
    fn reads_name_longer_than_one_report() {
        let name = "ABCDEFGHIJKLMNOPQRST";
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 3, fb(0), &[20]));
        fake.push_incoming(long_reply(1, 3, fb(1), &name.as_bytes()[..16]));
        fake.push_incoming(long_reply(1, 3, fb(1), &name.as_bytes()[16..]));
        assert_eq!(read(&mut dev(fake)), Ok(name.to_string()));
    }

    #[test]
    fn empty_chunks_do_not_loop_forever() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 3, fb(0), &[5]));
        fake.push_incoming(long_reply(1, 3, fb(1), &[]));
        assert_eq!(read(&mut dev(fake)), Ok(String::new()));
    }

    #[test]
    fn rejects_non_utf8() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 3, fb(0), &[2]));
        fake.push_incoming(long_reply(1, 3, fb(1), &[0xFF, 0xFE]));
        assert_eq!(
            read(&mut dev(fake)),
            Err(HidppError::UnexpectedResponse("device name is not UTF-8"))
        );
    }

    #[test]
    fn missing_feature_is_reported() {
        let mut d = Device::with_features(FakeTransport::new(), 1, HashMap::new());
        assert_eq!(read(&mut d), Err(HidppError::FeatureNotSupported(DEVICE_NAME)));
    }
}
```

Em `crates/hidpp/src/features/mod.rs`, trocar o início por:

```rust
pub mod battery;
pub mod device_name;
pub mod dpi;
pub mod report_rate;
```

- [ ] **Step 6: Rodar tudo**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: todos os testes passam (os 49 anteriores + 3 de hidraw + 1 de discover + 5 de device_name), sem avisos.

- [ ] **Step 7: Commit**

```bash
git add crates/hidpp
git commit -m "feat(hidpp): harden hidraw reads, ignore feature id 0, add device name feature"
```

---

### Task 2: Crate `ghub-ipc` (contrato D-Bus)

**Files:**
- Modify: `Cargo.toml` (workspace)
- Create: `crates/ghub-ipc/Cargo.toml`
- Create: `crates/ghub-ipc/src/lib.rs`

**Interfaces:**
- Produces (usado por `ghubd` e, depois, pela bandeja e pela GUI):
  - Constantes `BUS_NAME`, `OBJECT_PATH`, `INTERFACE`, `BATTERY_UNKNOWN = -1`, `STATE_UNKNOWN/DISCHARGING/CHARGING/FULL` (`&str`).
  - `DeviceSummary { id: String, name: String, connected: bool, battery_percent: i32, battery_state: String }` (assinatura D-Bus `(ssbis)`).
  - `DeviceState { id, name, connected, dpi_current: u32, dpi_levels: Vec<u32>, active_level: u32, dpi_min: u32, dpi_max: u32, dpi_step: u32, dpi_list: Vec<u32>, polling_hz: u32, polling_supported: Vec<u32>, battery_percent: i32, battery_state: String, profile: String }`.
  - Trait de proxy `Daemon` (gera `DaemonProxy` e `DaemonProxyBlocking`) com os métodos e sinais abaixo.

- [ ] **Step 1: Workspace e manifesto**

`Cargo.toml` (raiz): trocar `members` por

```toml
members = ["crates/hidpp", "crates/ghubctl", "crates/ghub-ipc", "crates/ghubd"]
```

(`crates/ghubd` ainda não existe: criar na Task 3; para esta task compilar, usar temporariamente `members = ["crates/hidpp", "crates/ghubctl", "crates/ghub-ipc"]` e acrescentar `ghubd` na Task 3.)

`crates/ghub-ipc/Cargo.toml`:

```toml
[package]
name = "ghub-ipc"
version.workspace = true
edition.workspace = true
publish.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
zbus = "5"
zvariant = "5"
```

- [ ] **Step 2: Escrever os testes que falham**

`crates/ghub-ipc/src/lib.rs` (só os testes e as constantes mínimas para compilar os testes):

```rust
//! Contrato D-Bus entre o daemon `ghubd` e seus clientes (GUI, bandeja, CLI).

use serde::{Deserialize, Serialize};
use zbus::proxy;
use zvariant::Type;

#[cfg(test)]
mod tests {
    use super::*;
    use zvariant::serialized::Context;
    use zvariant::{to_bytes, LE};

    fn roundtrip<T>(value: &T) -> T
    where
        T: serde::Serialize + serde::de::DeserializeOwned + Type,
    {
        let bytes = to_bytes(Context::new_dbus(LE, 0), value).unwrap();
        let (back, _): (T, usize) = bytes.deserialize().unwrap();
        back
    }

    #[test]
    fn summary_signature_is_stable() {
        assert_eq!(DeviceSummary::SIGNATURE.to_string(), "(ssbis)");
    }

    #[test]
    fn summary_roundtrips() {
        let v = DeviceSummary {
            id: "046dc539-1".into(),
            name: "G703 LS".into(),
            connected: true,
            battery_percent: BATTERY_UNKNOWN,
            battery_state: STATE_UNKNOWN.into(),
        };
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn state_roundtrips() {
        let v = DeviceState {
            id: "046dc539-1".into(),
            name: "G703 LS".into(),
            connected: true,
            dpi_current: 1200,
            dpi_levels: vec![400, 800, 1600],
            active_level: 2,
            dpi_min: 100,
            dpi_max: 25600,
            dpi_step: 50,
            dpi_list: vec![],
            polling_hz: 1000,
            polling_supported: vec![125, 250, 500, 1000],
            battery_percent: 91,
            battery_state: STATE_DISCHARGING.into(),
            profile: "Padrão".into(),
        };
        assert_eq!(roundtrip(&v), v);
    }

    #[test]
    fn constants_match_the_spec() {
        assert_eq!(BUS_NAME, "org.ghublinux.Daemon");
        assert_eq!(OBJECT_PATH, "/org/ghublinux/Daemon");
        assert_eq!(INTERFACE, "org.ghublinux.Daemon1");
    }
}
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p ghub-ipc`
Expected: FAIL de compilação (`DeviceSummary`, constantes não definidos).

- [ ] **Step 4: Implementar**

Inserir em `crates/ghub-ipc/src/lib.rs`, entre os `use` e o bloco de testes:

```rust
pub const BUS_NAME: &str = "org.ghublinux.Daemon";
pub const OBJECT_PATH: &str = "/org/ghublinux/Daemon";
pub const INTERFACE: &str = "org.ghublinux.Daemon1";

/// Valor de `battery_percent` quando a bateria é desconhecida.
pub const BATTERY_UNKNOWN: i32 = -1;

pub const STATE_UNKNOWN: &str = "unknown";
pub const STATE_DISCHARGING: &str = "discharging";
pub const STATE_CHARGING: &str = "charging";
pub const STATE_FULL: &str = "full";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DeviceSummary {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub battery_percent: i32,
    pub battery_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct DeviceState {
    pub id: String,
    pub name: String,
    pub connected: bool,
    /// DPI lido do dispositivo agora; 0 se desconhecido (offline).
    pub dpi_current: u32,
    pub dpi_levels: Vec<u32>,
    pub active_level: u32,
    pub dpi_min: u32,
    pub dpi_max: u32,
    /// 0 quando o dispositivo anuncia uma lista explícita (ver `dpi_list`).
    pub dpi_step: u32,
    pub dpi_list: Vec<u32>,
    pub polling_hz: u32,
    pub polling_supported: Vec<u32>,
    pub battery_percent: i32,
    pub battery_state: String,
    pub profile: String,
}

#[proxy(
    interface = "org.ghublinux.Daemon1",
    default_service = "org.ghublinux.Daemon",
    default_path = "/org/ghublinux/Daemon"
)]
pub trait Daemon {
    fn devices(&self) -> zbus::Result<Vec<DeviceSummary>>;
    fn get_device_state(&self, id: &str) -> zbus::Result<DeviceState>;
    fn set_dpi_levels(&self, id: &str, levels: Vec<u32>, active: u32) -> zbus::Result<()>;
    fn set_active_dpi_level(&self, id: &str, active: u32) -> zbus::Result<()>;
    fn set_polling_rate(&self, id: &str, hz: u32) -> zbus::Result<()>;
    fn list_profiles(&self) -> zbus::Result<Vec<String>>;
    fn get_active_profile(&self) -> zbus::Result<String>;
    fn activate_profile(&self, name: &str) -> zbus::Result<()>;
    fn create_profile(&self, name: &str) -> zbus::Result<()>;
    fn delete_profile(&self, name: &str) -> zbus::Result<()>;
    fn rescan(&self) -> zbus::Result<()>;

    #[zbus(signal)]
    fn devices_changed(&self) -> zbus::Result<()>;
    #[zbus(signal)]
    fn device_changed(&self, id: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn profile_changed(&self, name: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn battery_low(&self, id: String, percent: u32) -> zbus::Result<()>;
}
```

- [ ] **Step 5: Rodar e ver passar**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ghub-ipc
git commit -m "feat(ghub-ipc): add D-Bus contract types and proxy"
```

---

### Task 3: Crate `ghubd` e armazenamento de perfis

**Files:**
- Modify: `Cargo.toml` (acrescentar `crates/ghubd` aos membros)
- Create: `crates/ghubd/Cargo.toml`
- Create: `crates/ghubd/src/lib.rs`
- Create: `crates/ghubd/src/main.rs`
- Create: `crates/ghubd/src/profile.rs`

**Interfaces:**
- Consumes: `hidpp::features::dpi::DpiSupport`.
- Produces (módulo `ghubd::profile`):
  - `MAX_DPI_LEVELS = 5`, `DEFAULT_PROFILE = "Padrão"`.
  - `Profile { name: String, dpi_levels: Vec<u16>, active_level: usize, polling_hz: u16 }` com `Profile::new(&str)`, `active_dpi(&self) -> u16`, `check_shape(&self) -> Result<(), ProfileError>`.
  - `ProfileError { Invalid(String), NotFound(String), Exists(String) }` (`Display`, `Error`).
  - `dpi_allowed(support: &DpiSupport, dpi: u16) -> bool`.
  - `ProfileStore`: `default_store()`, `from_toml(&str) -> Result<Self, ProfileError>`, `to_toml(&self) -> String`, `load_or_default(&Path) -> (Self, Option<String>)`, `save(&self, &Path) -> io::Result<()>`, `active(&self) -> &Profile`, `active_name(&self) -> &str`, `names(&self) -> Vec<String>`, `activate(&mut self, &str) -> Result<(), ProfileError>`, `create(&mut self, &str) -> Result<(), ProfileError>` (copia o ativo), `delete(&mut self, &str) -> Result<(), ProfileError>`, `update_active(&mut self, Profile)`, `needs_seed(&self) -> bool`, `seed(&mut self, dpi: u16, polling_hz: u16)`.
  - `store_path_from(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf>`.

- [ ] **Step 1: Esqueleto do crate**

`Cargo.toml` (raiz): `members = ["crates/hidpp", "crates/ghubctl", "crates/ghub-ipc", "crates/ghubd"]`.

`crates/ghubd/Cargo.toml`:

```toml
[package]
name = "ghubd"
version.workspace = true
edition.workspace = true
publish.workspace = true

[dependencies]
hidpp = { path = "../hidpp" }
ghub-ipc = { path = "../ghub-ipc" }
serde = { version = "1", features = ["derive"] }
toml = "1"
zbus = "5"
udev = "0.9"
libc = "0.2"

[dev-dependencies]
tempfile = "3"
```

`crates/ghubd/src/lib.rs`:

```rust
//! Lógica do daemon `ghubd`.

pub mod profile;
```

`crates/ghubd/src/main.rs`:

```rust
fn main() {
    println!("ghubd");
}
```

- [ ] **Step 2: Escrever os testes que falham**

`crates/ghubd/src/profile.rs` (somente testes e `use`; a implementação vem no Step 4):

```rust
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use hidpp::features::dpi::DpiSupport;
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, levels: &[u16], active: usize, hz: u16) -> Profile {
        Profile {
            name: name.into(),
            dpi_levels: levels.to_vec(),
            active_level: active,
            polling_hz: hz,
        }
    }

    #[test]
    fn default_store_is_valid_and_needs_seed() {
        let s = ProfileStore::default_store();
        assert!(s.needs_seed());
        assert_eq!(s.active_name(), DEFAULT_PROFILE);
        assert_eq!(s.active().check_shape(), Ok(()));
        assert_eq!(s.active().active_dpi(), 1600);
    }

    #[test]
    fn seed_replaces_levels_with_device_values_and_clears_flag() {
        let mut s = ProfileStore::default_store();
        s.seed(1200, 500);
        assert!(!s.needs_seed());
        assert_eq!(s.active().dpi_levels, vec![1200]);
        assert_eq!(s.active().active_level, 0);
        assert_eq!(s.active().polling_hz, 500);
    }

    #[test]
    fn toml_roundtrip() {
        let mut s = ProfileStore::default_store();
        s.seed(1200, 1000);
        s.create("Trabalho").unwrap();
        let text = s.to_toml();
        let back = ProfileStore::from_toml(&text).unwrap();
        assert_eq!(back.names(), vec![DEFAULT_PROFILE.to_string(), "Trabalho".to_string()]);
        assert_eq!(back.active().dpi_levels, vec![1200]);
        assert!(!back.needs_seed());
    }

    #[test]
    fn from_toml_rejects_bad_files() {
        assert!(ProfileStore::from_toml("not toml {{{").is_err());
        assert!(ProfileStore::from_toml("active = \"x\"\n").is_err()); // sem perfis
        let dup = "active = \"a\"\n[[profile]]\nname=\"a\"\ndpi_levels=[400]\nactive_level=0\npolling_hz=1000\n[[profile]]\nname=\"a\"\ndpi_levels=[400]\nactive_level=0\npolling_hz=1000\n";
        assert!(ProfileStore::from_toml(dup).is_err());
        let missing_active = "active = \"b\"\n[[profile]]\nname=\"a\"\ndpi_levels=[400]\nactive_level=0\npolling_hz=1000\n";
        assert!(ProfileStore::from_toml(missing_active).is_err());
        let bad_level = "active = \"a\"\n[[profile]]\nname=\"a\"\ndpi_levels=[400]\nactive_level=3\npolling_hz=1000\n";
        assert!(ProfileStore::from_toml(bad_level).is_err());
    }

    #[test]
    fn check_shape_rejects_bad_profiles() {
        assert!(profile("ok", &[400], 0, 1000).check_shape().is_ok());
        assert!(profile("", &[400], 0, 1000).check_shape().is_err());
        assert!(profile("   ", &[400], 0, 1000).check_shape().is_err());
        assert!(profile(&"x".repeat(65), &[400], 0, 1000).check_shape().is_err());
        assert!(profile("a\nb", &[400], 0, 1000).check_shape().is_err());
        assert!(profile("p", &[], 0, 1000).check_shape().is_err());
        assert!(profile("p", &[1, 2, 3, 4, 5, 6], 0, 1000).check_shape().is_err());
        assert!(profile("p", &[400], 1, 1000).check_shape().is_err());
        assert!(profile("p", &[0], 0, 1000).check_shape().is_err());
        assert!(profile("p", &[400], 0, 0).check_shape().is_err());
    }

    #[test]
    fn create_copies_active_and_rejects_duplicates_and_bad_names() {
        let mut s = ProfileStore::default_store();
        s.seed(800, 1000);
        s.create("Trabalho").unwrap();
        assert_eq!(s.get("Trabalho").unwrap().dpi_levels, vec![800]);
        assert!(matches!(s.create("Trabalho"), Err(ProfileError::Exists(_))));
        assert!(matches!(s.create("  "), Err(ProfileError::Invalid(_))));
        assert_eq!(s.active_name(), DEFAULT_PROFILE); // criar não ativa
    }

    #[test]
    fn activate_and_delete_rules() {
        let mut s = ProfileStore::default_store();
        s.create("B").unwrap();
        assert!(matches!(s.activate("zzz"), Err(ProfileError::NotFound(_))));
        s.activate("B").unwrap();
        assert_eq!(s.active_name(), "B");
        assert!(matches!(s.delete("B"), Err(ProfileError::Invalid(_)))); // ativo
        s.delete(DEFAULT_PROFILE).unwrap();
        assert!(matches!(s.delete(DEFAULT_PROFILE), Err(ProfileError::NotFound(_))));
        assert!(matches!(s.delete("B"), Err(ProfileError::Invalid(_)))); // último
    }

    #[test]
    fn update_active_keeps_the_name() {
        let mut s = ProfileStore::default_store();
        s.update_active(profile("outro nome", &[800, 1600], 1, 500));
        assert_eq!(s.active_name(), DEFAULT_PROFILE);
        assert_eq!(s.active().dpi_levels, vec![800, 1600]);
        assert_eq!(s.active().polling_hz, 500);
    }

    #[test]
    fn dpi_allowed_checks_list_and_range() {
        let list = DpiSupport::List(vec![400, 800]);
        assert!(dpi_allowed(&list, 800));
        assert!(!dpi_allowed(&list, 900));
        let range = DpiSupport::Range { min: 100, max: 25600, step: 50 };
        assert!(dpi_allowed(&range, 100));
        assert!(dpi_allowed(&range, 1650));
        assert!(dpi_allowed(&range, 25600));
        assert!(!dpi_allowed(&range, 125));
        assert!(!dpi_allowed(&range, 50));
        assert!(!dpi_allowed(&range, 25650));
    }

    #[test]
    fn missing_file_gives_default_without_warning() {
        let dir = tempfile::tempdir().unwrap();
        let (s, warning) = ProfileStore::load_or_default(&dir.path().join("profiles.toml"));
        assert!(s.needs_seed());
        assert_eq!(warning, None);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_default_is_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        fs::write(&path, "isto não é toml {{{").unwrap();
        let (s, warning) = ProfileStore::load_or_default(&path);
        assert!(s.needs_seed());
        assert!(warning.unwrap().contains("profiles.toml.bak"));
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("profiles.toml.bak")).unwrap(),
            "isto não é toml {{{"
        );
    }

    #[test]
    fn empty_file_is_treated_as_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        fs::write(&path, "").unwrap();
        let (_, warning) = ProfileStore::load_or_default(&path);
        assert!(warning.is_some());
    }

    #[test]
    fn save_is_atomic_and_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/profiles.toml");
        let mut s = ProfileStore::default_store();
        s.seed(1200, 1000);
        s.save(&path).unwrap();
        assert!(!dir.path().join("sub/profiles.toml.tmp").exists());
        let (back, warning) = ProfileStore::load_or_default(&path);
        assert_eq!(warning, None);
        assert_eq!(back.active().dpi_levels, vec![1200]);
    }

    #[test]
    fn store_path_resolution() {
        assert_eq!(
            store_path_from(Some("/x"), Some("/home/u")),
            Some(PathBuf::from("/x/ghub-linux/profiles.toml"))
        );
        assert_eq!(
            store_path_from(None, Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/ghub-linux/profiles.toml"))
        );
        assert_eq!(
            store_path_from(Some(""), Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/ghub-linux/profiles.toml"))
        );
        assert_eq!(store_path_from(None, None), None);
    }
}
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p ghubd`
Expected: FAIL de compilação (`Profile`, `ProfileStore`… não definidos).

- [ ] **Step 4: Implementar**

Inserir em `profile.rs`, entre os `use` e o bloco de testes:

```rust
pub const MAX_DPI_LEVELS: usize = 5;
pub const DEFAULT_PROFILE: &str = "Padrão";
const MAX_NAME_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    Invalid(String),
    NotFound(String),
    Exists(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::Invalid(m) => write!(f, "{m}"),
            ProfileError::NotFound(n) => write!(f, "perfil não encontrado: {n}"),
            ProfileError::Exists(n) => write!(f, "já existe um perfil chamado {n}"),
        }
    }
}

impl std::error::Error for ProfileError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub dpi_levels: Vec<u16>,
    pub active_level: usize,
    pub polling_hz: u16,
}

fn check_name(name: &str) -> Result<(), ProfileError> {
    if name.trim().is_empty() {
        return Err(ProfileError::Invalid("o nome do perfil não pode ser vazio".into()));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(ProfileError::Invalid(format!(
            "o nome do perfil excede {MAX_NAME_LEN} caracteres"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(ProfileError::Invalid(
            "o nome do perfil não pode ter caracteres de controle".into(),
        ));
    }
    Ok(())
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Profile {
            name: name.to_string(),
            dpi_levels: vec![400, 800, 1600, 3200, 6400],
            active_level: 2,
            polling_hz: 1000,
        }
    }

    pub fn active_dpi(&self) -> u16 {
        self.dpi_levels[self.active_level]
    }

    pub fn check_shape(&self) -> Result<(), ProfileError> {
        check_name(&self.name)?;
        if self.dpi_levels.is_empty() || self.dpi_levels.len() > MAX_DPI_LEVELS {
            return Err(ProfileError::Invalid(format!(
                "o perfil precisa de 1 a {MAX_DPI_LEVELS} níveis de DPI"
            )));
        }
        if self.dpi_levels.contains(&0) {
            return Err(ProfileError::Invalid("DPI 0 não é válido".into()));
        }
        if self.active_level >= self.dpi_levels.len() {
            return Err(ProfileError::Invalid(
                "o nível ativo está fora da lista de níveis".into(),
            ));
        }
        if self.polling_hz == 0 {
            return Err(ProfileError::Invalid("a taxa de polling não pode ser 0".into()));
        }
        Ok(())
    }
}

/// Verifica se `dpi` é um valor que o dispositivo anunciou como suportado.
pub fn dpi_allowed(support: &DpiSupport, dpi: u16) -> bool {
    match support {
        DpiSupport::List(values) => values.contains(&dpi),
        DpiSupport::Range { min, max, step } => {
            dpi >= *min && dpi <= *max && (*step == 0 || (dpi - *min).is_multiple_of(*step))
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    active: String,
    #[serde(default)]
    profile: Vec<Profile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileStore {
    active: String,
    profiles: Vec<Profile>,
    needs_seed: bool,
}

impl ProfileStore {
    pub fn default_store() -> Self {
        ProfileStore {
            active: DEFAULT_PROFILE.to_string(),
            profiles: vec![Profile::new(DEFAULT_PROFILE)],
            needs_seed: true,
        }
    }

    pub fn from_toml(text: &str) -> Result<Self, ProfileError> {
        let file: StoreFile = toml::from_str(text)
            .map_err(|e| ProfileError::Invalid(format!("TOML inválido: {e}")))?;
        if file.profile.is_empty() {
            return Err(ProfileError::Invalid("o arquivo não tem perfis".into()));
        }
        let mut seen: Vec<&str> = Vec::new();
        for p in &file.profile {
            p.check_shape()?;
            if seen.contains(&p.name.as_str()) {
                return Err(ProfileError::Invalid(format!("perfil duplicado: {}", p.name)));
            }
            seen.push(&p.name);
        }
        if !file.profile.iter().any(|p| p.name == file.active) {
            return Err(ProfileError::Invalid(format!(
                "o perfil ativo não existe: {}",
                file.active
            )));
        }
        Ok(ProfileStore {
            active: file.active,
            profiles: file.profile,
            needs_seed: false,
        })
    }

    pub fn to_toml(&self) -> String {
        let file = StoreFile {
            active: self.active.clone(),
            profile: self.profiles.clone(),
        };
        toml::to_string_pretty(&file).expect("os perfis são serializáveis")
    }

    /// Lê o arquivo. Se faltar, devolve o padrão. Se for inválido, guarda uma cópia
    /// `.bak`, devolve o padrão e um aviso.
    pub fn load_or_default(path: &Path) -> (Self, Option<String>) {
        match fs::read_to_string(path) {
            Ok(text) => match Self::from_toml(&text) {
                Ok(store) => (store, None),
                Err(e) => {
                    let bak = path.with_extension("toml.bak");
                    let _ = fs::rename(path, &bak);
                    (
                        Self::default_store(),
                        Some(format!(
                            "{} inválido ({e}); cópia em {}",
                            path.display(),
                            bak.display()
                        )),
                    )
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::default_store(), None),
            Err(e) => (
                Self::default_store(),
                Some(format!("não foi possível ler {}: {e}", path.display())),
            ),
        }
    }

    /// Escrita atômica: arquivo temporário, `fsync` e `rename`.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        let mut file = fs::File::create(&tmp)?;
        file.write_all(self.to_toml().as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    }

    pub fn active(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.name == self.active)
            .expect("o perfil ativo sempre existe")
    }

    pub fn active_name(&self) -> &str {
        &self.active
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn names(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.name.clone()).collect()
    }

    pub fn activate(&mut self, name: &str) -> Result<(), ProfileError> {
        if self.get(name).is_none() {
            return Err(ProfileError::NotFound(name.to_string()));
        }
        self.active = name.to_string();
        Ok(())
    }

    /// Cria um perfil copiando o ativo. Não o ativa.
    pub fn create(&mut self, name: &str) -> Result<(), ProfileError> {
        check_name(name)?;
        if self.get(name).is_some() {
            return Err(ProfileError::Exists(name.to_string()));
        }
        let mut copy = self.active().clone();
        copy.name = name.to_string();
        self.profiles.push(copy);
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Result<(), ProfileError> {
        if self.get(name).is_none() {
            return Err(ProfileError::NotFound(name.to_string()));
        }
        if name == self.active {
            return Err(ProfileError::Invalid(
                "não é possível apagar o perfil ativo".into(),
            ));
        }
        if self.profiles.len() == 1 {
            return Err(ProfileError::Invalid(
                "não é possível apagar o último perfil".into(),
            ));
        }
        self.profiles.retain(|p| p.name != name);
        Ok(())
    }

    /// Substitui o conteúdo do perfil ativo, preservando o nome.
    pub fn update_active(&mut self, mut profile: Profile) {
        profile.name = self.active.clone();
        if let Some(slot) = self.profiles.iter_mut().find(|p| p.name == self.active) {
            *slot = profile;
        }
    }

    pub fn needs_seed(&self) -> bool {
        self.needs_seed
    }

    /// Semeia o perfil ativo com os valores atuais do dispositivo (primeira execução).
    pub fn seed(&mut self, dpi: u16, polling_hz: u16) {
        let mut seeded = self.active().clone();
        seeded.dpi_levels = vec![dpi];
        seeded.active_level = 0;
        seeded.polling_hz = polling_hz;
        self.update_active(seeded);
        self.needs_seed = false;
    }
}

/// `$XDG_CONFIG_HOME/ghub-linux/profiles.toml`, ou `$HOME/.config/ghub-linux/profiles.toml`.
pub fn store_path_from(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg.filter(|v| !v.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(home.filter(|v| !v.is_empty())?).join(".config"),
    };
    Some(base.join("ghub-linux").join("profiles.toml"))
}
```

- [ ] **Step 5: Rodar e ver passar**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos (testes de `profile` todos verdes).

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock crates/ghubd
git commit -m "feat(ghubd): add profile store with atomic TOML persistence"
```

---

### Task 4: Topologia sysfs e bateria do kernel

**Files:**
- Create: `crates/ghubd/src/topology.rs`
- Create: `crates/ghubd/src/battery_sysfs.rs`
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Consumes: `hidpp::enumerate::{parse_uevent, has_hidpp_collection}`, `ghub_ipc::{STATE_*}`.
- Produces:
  - `topology::HidNode { devnode: PathBuf, hid_dir: PathBuf, vendor_id: u16, product_id: u16, name: String }`; `topology::top_level_nodes(sysfs: &Path, dev: &Path) -> io::Result<Vec<HidNode>>` (só nós HID++ da Logitech cujo pai **não** é um dispositivo HID; ordenados por `devnode`).
  - `battery_sysfs::{ChargeStatus {Unknown, Discharging, Charging, Full} (com as_str() -> &'static str), BatteryInfo { percent: u8, status: ChargeStatus, voltage_mv: Option<u32> }, find_batteries(hid_dir: &Path) -> Vec<PathBuf>, read_battery(dir: &Path) -> Option<BatteryInfo>}`.

- [ ] **Step 1: Escrever os testes que falham (topologia)**

`crates/ghubd/src/topology.rs` (só `use` e testes):

```rust
use std::fs;
use std::path::{Path, PathBuf};

use hidpp::enumerate::{has_hidpp_collection, parse_uevent};

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const HIDPP_DESC: [u8; 9] = [0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x11];
    const KEYBOARD_DESC: [u8; 8] = [0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01];

    fn make_hid(dir: &Path, vid: &str, pid: &str, name: &str, desc: &[u8]) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("uevent"),
            format!("HID_ID=0003:0000{vid}:0000{pid}\nHID_NAME={name}\n"),
        )
        .unwrap();
        fs::write(dir.join("report_descriptor"), desc).unwrap();
    }

    fn link_hidraw(root: &Path, node: &str, hid_dir: &Path) {
        let d = root.join("class/hidraw").join(node);
        fs::create_dir_all(&d).unwrap();
        symlink(hid_dir, d.join("device")).unwrap();
    }

    /// Reproduz a árvore real: receptor (interface HID++ e uma de teclado) e o mouse como filho.
    fn fixture() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let usb = t.path().join("devices/usb/3-4");
        let hidpp = usb.join("3-4:1.2/0003:046D:C539.0005");
        let kbd = usb.join("3-4:1.1/0003:046D:C539.0003");
        let child = hidpp.join("0003:046D:4086.0009");
        make_hid(&hidpp, "046D", "C539", "Logitech USB Receiver", &HIDPP_DESC);
        make_hid(&kbd, "046D", "C539", "Logitech USB Receiver", &KEYBOARD_DESC);
        make_hid(&child, "046D", "4086", "Logitech G703 LS", &HIDPP_DESC);
        link_hidraw(t.path(), "hidraw4", &hidpp);
        link_hidraw(t.path(), "hidraw3", &kbd);
        link_hidraw(t.path(), "hidraw8", &child);
        t
    }

    #[test]
    fn keeps_only_top_level_hidpp_nodes() {
        let t = fixture();
        let nodes = top_level_nodes(t.path(), Path::new("/dev")).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].devnode, PathBuf::from("/dev/hidraw4"));
        assert_eq!(nodes[0].vendor_id, 0x046d);
        assert_eq!(nodes[0].product_id, 0xc539);
        assert_eq!(nodes[0].name, "Logitech USB Receiver");
        assert!(nodes[0].hid_dir.ends_with("0003:046D:C539.0005"));
    }

    #[test]
    fn ignores_non_logitech_nodes() {
        let t = fixture();
        let other = t.path().join("devices/usb/1-1/1-1:1.0/0003:1234:5678.0001");
        make_hid(&other, "1234", "5678", "Outro", &HIDPP_DESC);
        link_hidraw(t.path(), "hidraw1", &other);
        let nodes = top_level_nodes(t.path(), Path::new("/dev")).unwrap();
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn missing_class_dir_is_an_error() {
        let t = tempfile::tempdir().unwrap();
        assert!(top_level_nodes(t.path(), Path::new("/dev")).is_err());
    }

    #[test]
    fn broken_symlink_is_skipped() {
        let t = fixture();
        let d = t.path().join("class/hidraw/hidraw9");
        fs::create_dir_all(&d).unwrap();
        symlink(t.path().join("nao/existe"), d.join("device")).unwrap();
        let nodes = top_level_nodes(t.path(), Path::new("/dev")).unwrap();
        assert_eq!(nodes.len(), 1);
    }
}
```

- [ ] **Step 2: Testes que falham (bateria)**

`crates/ghubd/src/battery_sysfs.rs` (só `use` e testes):

```rust
use std::fs;
use std::path::{Path, PathBuf};

use ghub_ipc::{STATE_CHARGING, STATE_DISCHARGING, STATE_FULL, STATE_UNKNOWN};

#[cfg(test)]
mod tests {
    use super::*;

    fn write_battery(dir: &Path, kind: &str, capacity: &str, status: &str, voltage: Option<&str>) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("type"), format!("{kind}\n")).unwrap();
        fs::write(dir.join("capacity"), format!("{capacity}\n")).unwrap();
        fs::write(dir.join("status"), format!("{status}\n")).unwrap();
        if let Some(v) = voltage {
            fs::write(dir.join("voltage_now"), format!("{v}\n")).unwrap();
        }
    }

    #[test]
    fn reads_the_real_g703_values() {
        let t = tempfile::tempdir().unwrap();
        write_battery(t.path(), "Battery", "91", "Discharging", Some("4083000"));
        assert_eq!(
            read_battery(t.path()),
            Some(BatteryInfo {
                percent: 91,
                status: ChargeStatus::Discharging,
                voltage_mv: Some(4083),
            })
        );
    }

    #[test]
    fn maps_statuses_and_tolerates_missing_voltage() {
        let t = tempfile::tempdir().unwrap();
        for (raw, expected) in [
            ("Charging", ChargeStatus::Charging),
            ("Full", ChargeStatus::Full),
            ("Not charging", ChargeStatus::Unknown),
            ("whatever", ChargeStatus::Unknown),
        ] {
            write_battery(t.path(), "Battery", "50", raw, None);
            let b = read_battery(t.path()).unwrap();
            assert_eq!(b.status, expected, "{raw}");
            assert_eq!(b.voltage_mv, None);
        }
    }

    #[test]
    fn rejects_bad_capacity() {
        let t = tempfile::tempdir().unwrap();
        for bad in ["abc", "101", "-3", ""] {
            write_battery(t.path(), "Battery", bad, "Discharging", None);
            assert_eq!(read_battery(t.path()), None, "{bad}");
        }
        assert_eq!(read_battery(&t.path().join("nao-existe")), None);
    }

    #[test]
    fn status_strings_match_the_ipc_contract() {
        assert_eq!(ChargeStatus::Unknown.as_str(), STATE_UNKNOWN);
        assert_eq!(ChargeStatus::Discharging.as_str(), STATE_DISCHARGING);
        assert_eq!(ChargeStatus::Charging.as_str(), STATE_CHARGING);
        assert_eq!(ChargeStatus::Full.as_str(), STATE_FULL);
    }

    #[test]
    fn finds_batteries_under_child_hid_devices_only_of_type_battery() {
        let t = tempfile::tempdir().unwrap();
        let hid = t.path().join("0003:046D:C539.0005");
        let child = hid.join("0003:046D:4086.0009/power_supply");
        write_battery(&child.join("hidpp_battery_0"), "Battery", "91", "Discharging", None);
        write_battery(&child.join("ac"), "Mains", "1", "Unknown", None);
        let found = find_batteries(&hid);
        assert_eq!(found, vec![child.join("hidpp_battery_0")]);
    }

    #[test]
    fn find_batteries_on_missing_dir_is_empty() {
        let t = tempfile::tempdir().unwrap();
        assert!(find_batteries(&t.path().join("nada")).is_empty());
    }
}
```

- [ ] **Step 3: Rodar e ver falhar**

Em `lib.rs` acrescentar `pub mod battery_sysfs;` e `pub mod topology;`.

Run: `cargo test -p ghubd`
Expected: FAIL de compilação (`top_level_nodes`, `read_battery`… não definidos).

- [ ] **Step 4: Implementar a topologia**

Inserir em `topology.rs`, entre os `use` e os testes:

```rust
const LOGITECH_VENDOR: u16 = 0x046d;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidNode {
    /// Caminho do nó, por exemplo `/dev/hidraw4`.
    pub devnode: PathBuf,
    /// Diretório do dispositivo HID no sysfs (canônico).
    pub hid_dir: PathBuf,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
}

/// Nome de dispositivo HID no sysfs, por exemplo `0003:046D:C539.0005`.
fn is_hid_name(name: &str) -> bool {
    name.len() == 19
        && name.char_indices().all(|(i, c)| match i {
            4 | 9 => c == ':',
            14 => c == '.',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Nós hidraw HID++ da Logitech que **não** são filhos de outro dispositivo HID.
///
/// O receptor Lightspeed aparece como nó de topo; o mouse pareado aparece como nó filho
/// do `hid-logitech-dj` e é ignorado aqui (o daemon fala com o mouse pelo nó do receptor).
pub fn top_level_nodes(sysfs: &Path, dev: &Path) -> std::io::Result<Vec<HidNode>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(sysfs.join("class/hidraw"))? {
        let entry = entry?;
        let node = entry.file_name().to_string_lossy().into_owned();
        let Ok(hid_dir) = fs::canonicalize(entry.path().join("device")) else {
            continue;
        };
        let parent_is_hid = hid_dir
            .parent()
            .and_then(Path::file_name)
            .map(|n| is_hid_name(&n.to_string_lossy()))
            .unwrap_or(false);
        if parent_is_hid {
            continue;
        }
        let Ok(uevent) = fs::read_to_string(hid_dir.join("uevent")) else {
            continue;
        };
        let Some((vendor_id, product_id, name)) = parse_uevent(&uevent) else {
            continue;
        };
        if vendor_id != LOGITECH_VENDOR {
            continue;
        }
        let Ok(descriptor) = fs::read(hid_dir.join("report_descriptor")) else {
            continue;
        };
        if !has_hidpp_collection(&descriptor) {
            continue;
        }
        out.push(HidNode {
            devnode: dev.join(node),
            hid_dir,
            vendor_id,
            product_id,
            name,
        });
    }
    out.sort_by(|a, b| a.devnode.cmp(&b.devnode));
    Ok(out)
}
```

- [ ] **Step 5: Implementar a bateria**

Inserir em `battery_sysfs.rs`, entre os `use` e os testes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeStatus {
    Unknown,
    Discharging,
    Charging,
    Full,
}

impl ChargeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ChargeStatus::Unknown => STATE_UNKNOWN,
            ChargeStatus::Discharging => STATE_DISCHARGING,
            ChargeStatus::Charging => STATE_CHARGING,
            ChargeStatus::Full => STATE_FULL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryInfo {
    pub percent: u8,
    pub status: ChargeStatus,
    pub voltage_mv: Option<u32>,
}

/// Procura baterias `power_supply` de dispositivos HID abaixo de `hid_dir`
/// (e do próprio `hid_dir`). Só considera entradas com `type` igual a `Battery`.
pub fn find_batteries(hid_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![hid_dir.join("power_supply")];
    if let Ok(children) = fs::read_dir(hid_dir) {
        for child in children.flatten() {
            candidates.push(child.path().join("power_supply"));
        }
    }
    let mut out = Vec::new();
    for ps in candidates {
        let Ok(entries) = fs::read_dir(&ps) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let is_battery = fs::read_to_string(p.join("type"))
                .map(|t| t.trim() == "Battery")
                .unwrap_or(false);
            if is_battery {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

pub fn read_battery(dir: &Path) -> Option<BatteryInfo> {
    let percent: u8 = fs::read_to_string(dir.join("capacity"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if percent > 100 {
        return None;
    }
    let status = match fs::read_to_string(dir.join("status"))
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some("Charging") => ChargeStatus::Charging,
        Some("Discharging") => ChargeStatus::Discharging,
        Some("Full") => ChargeStatus::Full,
        _ => ChargeStatus::Unknown,
    };
    let voltage_mv = fs::read_to_string(dir.join("voltage_now"))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|uv| (uv / 1000) as u32);
    Some(BatteryInfo {
        percent,
        status,
        voltage_mv,
    })
}
```

- [ ] **Step 6: Rodar e ver passar**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos.

- [ ] **Step 7: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): add sysfs topology scan and kernel battery reader"
```

---

### Task 5: Trait `Mouse`, `HidppMouse` e `Backend`

**Files:**
- Create: `crates/ghubd/src/mouse.rs`
- Create: `crates/ghubd/src/backend.rs`
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Consumes: `hidpp::{device::{Device, DEFAULT_TIMEOUT}, features::{device_name, dpi::{self, DpiSupport}, report_rate}, hidraw::HidrawTransport, transport::Transport, HidppError}`; Task 4 (`top_level_nodes`, `HidNode`, `find_batteries`, `read_battery`, `BatteryInfo`).
- Produces:
  - `mouse::{MouseResult<T> = Result<T, HidppError>, Capabilities { dpi: DpiSupport, polling_hz: Vec<u16> }, trait Mouse: Send { capabilities, read_dpi, read_polling_hz, set_dpi(u16) -> MouseResult<u16>, set_polling_hz(u16) -> MouseResult<()>, ping }, HidppMouse<T>::{new(Device<T>), into_device(self)}}`.
  - `backend::{Found { id: String, name: String, mouse: Box<dyn Mouse> }, trait Backend: Send { scan(&mut self) -> Vec<Found>; battery(&mut self, id: &str) -> Option<BatteryInfo> }, SysBackend::new(), pairing(found_ids: &[String], batteries: &[PathBuf]) -> Option<(String, PathBuf)>}`.

- [ ] **Step 1: Escrever os testes que falham (`mouse.rs`)**

`crates/ghubd/src/mouse.rs`:

```rust
use std::time::Duration;

use hidpp::device::{Device, DEFAULT_TIMEOUT};
use hidpp::features::dpi::{self, DpiSupport};
use hidpp::features::report_rate;
use hidpp::transport::Transport;
use hidpp::HidppError;

#[cfg(test)]
mod tests {
    use super::*;
    use hidpp::transport::{fb, long_reply, FakeTransport};
    use std::collections::HashMap;

    fn mouse(fake: FakeTransport) -> HidppMouse<FakeTransport> {
        let features = HashMap::from([(0x2201u16, 11u8), (0x8060u16, 10u8)]);
        HidppMouse::new(Device::with_features(fake, 1, features))
    }

    #[test]
    fn reads_capabilities() {
        let mut fake = FakeTransport::new();
        // DPI: faixa 100..25600 passo 50.
        fake.push_incoming(long_reply(1, 11, fb(1), &[0, 0x00, 0x64, 0xE0, 0x32, 0x64, 0x00]));
        // Polling: bits 0,1,3,7 -> 1000/500/250/125 Hz.
        fake.push_incoming(long_reply(1, 10, fb(0), &[0b1000_1011]));
        let caps = mouse(fake).capabilities().unwrap();
        assert_eq!(caps.dpi, DpiSupport::Range { min: 100, max: 25600, step: 50 });
        assert_eq!(caps.polling_hz, vec![1000, 500, 250, 125]);
    }

    #[test]
    fn reads_current_values() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 11, fb(2), &[0, 0x04, 0xB0, 0x03, 0x20])); // 1200 / 800
        fake.push_incoming(long_reply(1, 10, fb(1), &[1]));
        let mut m = mouse(fake);
        assert_eq!(m.read_dpi(), Ok(1200));
        assert_eq!(m.read_polling_hz(), Ok(1000));
    }

    #[test]
    fn sets_dpi_and_polling() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 11, fb(3), &[0, 0x0C, 0x80]));
        fake.push_incoming(long_reply(1, 10, fb(2), &[2]));
        let mut m = mouse(fake);
        assert_eq!(m.set_dpi(3200), Ok(3200));
        assert_eq!(m.set_polling_hz(500), Ok(()));
        let t = m.into_device().into_transport();
        assert_eq!(t.written[0], vec![0x10, 1, 11, fb(3), 0, 0x0C, 0x80]);
        assert_eq!(t.written[1], vec![0x10, 1, 10, fb(2), 2, 0, 0]);
    }

    #[test]
    fn ping_succeeds_and_times_out() {
        let mut ok = FakeTransport::new();
        ok.push_incoming(long_reply(1, 0, fb(1), &[4, 2, 0xA5]));
        assert_eq!(mouse(ok).ping(), Ok(()));
        assert_eq!(mouse(FakeTransport::new()).ping(), Err(HidppError::Timeout));
    }
}
```

- [ ] **Step 2: Testes que falham (`backend.rs` — função `pairing`)**

`crates/ghubd/src/backend.rs` (só `use` e testes por enquanto):

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hidpp::device::{Device, DEFAULT_TIMEOUT};
use hidpp::features::device_name;
use hidpp::hidraw::HidrawTransport;

use crate::battery_sysfs::{find_batteries, read_battery, BatteryInfo};
use crate::mouse::{HidppMouse, Mouse};
use crate::topology::{top_level_nodes, HidNode};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_only_when_exactly_one_device_and_one_battery() {
        let ids = vec!["046dc539-1".to_string()];
        let bat = vec![PathBuf::from("/sys/x/hidpp_battery_0")];
        assert_eq!(
            pairing(&ids, &bat),
            Some(("046dc539-1".to_string(), PathBuf::from("/sys/x/hidpp_battery_0")))
        );
        assert_eq!(pairing(&ids, &[]), None);
        assert_eq!(pairing(&[], &bat), None);
        let two_ids = vec!["a".to_string(), "b".to_string()];
        assert_eq!(pairing(&two_ids, &bat), None);
        let two_bats = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        assert_eq!(pairing(&ids, &two_bats), None);
    }
}
```

- [ ] **Step 3: Rodar e ver falhar**

Em `lib.rs` acrescentar `pub mod backend;` e `pub mod mouse;`.

Run: `cargo test -p ghubd`
Expected: FAIL de compilação (`HidppMouse`, `pairing` não definidos).

- [ ] **Step 4: Implementar `mouse.rs`**

Inserir em `mouse.rs`, entre os `use` e os testes:

```rust
const PING_TIMEOUT: Duration = Duration::from_millis(500);
/// O G703 tem um único sensor.
const SENSOR: u8 = 0;

pub type MouseResult<T> = Result<T, HidppError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub dpi: DpiSupport,
    pub polling_hz: Vec<u16>,
}

/// Operações que o daemon precisa de um mouse. Implementado sobre HID++ e, nos testes, por um falso.
pub trait Mouse: Send {
    fn capabilities(&mut self) -> MouseResult<Capabilities>;
    fn read_dpi(&mut self) -> MouseResult<u16>;
    fn read_polling_hz(&mut self) -> MouseResult<u16>;
    /// Define o DPI e devolve o valor que o dispositivo aplicou.
    fn set_dpi(&mut self, dpi: u16) -> MouseResult<u16>;
    fn set_polling_hz(&mut self, hz: u16) -> MouseResult<()>;
    /// Verifica, sem demora, se o dispositivo ainda responde.
    fn ping(&mut self) -> MouseResult<()>;
}

pub struct HidppMouse<T: Transport> {
    device: Device<T>,
}

impl<T: Transport> HidppMouse<T> {
    pub fn new(device: Device<T>) -> Self {
        HidppMouse { device }
    }

    pub fn into_device(self) -> Device<T> {
        self.device
    }
}

impl<T: Transport + Send> Mouse for HidppMouse<T> {
    fn capabilities(&mut self) -> MouseResult<Capabilities> {
        Ok(Capabilities {
            dpi: dpi::supported(&mut self.device, SENSOR)?,
            polling_hz: report_rate::supported_hz(&mut self.device)?,
        })
    }

    fn read_dpi(&mut self) -> MouseResult<u16> {
        Ok(dpi::current(&mut self.device, SENSOR)?.0)
    }

    fn read_polling_hz(&mut self) -> MouseResult<u16> {
        report_rate::current_hz(&mut self.device)
    }

    fn set_dpi(&mut self, value: u16) -> MouseResult<u16> {
        dpi::set(&mut self.device, SENSOR, value)
    }

    fn set_polling_hz(&mut self, hz: u16) -> MouseResult<()> {
        report_rate::set_hz(&mut self.device, hz)
    }

    fn ping(&mut self) -> MouseResult<()> {
        self.device.set_timeout(PING_TIMEOUT);
        let result = self.device.ping();
        self.device.set_timeout(DEFAULT_TIMEOUT);
        result.map(|_| ())
    }
}
```

- [ ] **Step 5: Implementar `backend.rs`**

Inserir em `backend.rs`, entre os `use` e os testes:

```rust
/// Tempo máximo esperando a resposta de um slot do receptor durante a varredura.
const PROBE_TIMEOUT: Duration = Duration::from_millis(300);
/// Slots de dispositivo de um receptor Lightspeed/Unifying.
const SLOTS: std::ops::RangeInclusive<u8> = 1..=6;

pub struct Found {
    pub id: String,
    pub name: String,
    pub mouse: Box<dyn Mouse>,
}

/// Fonte de dispositivos e de bateria. A implementação real usa sysfs e hidraw.
pub trait Backend: Send {
    /// Abre todos os mouses que respondem agora.
    fn scan(&mut self) -> Vec<Found>;
    fn battery(&mut self, id: &str) -> Option<BatteryInfo>;
}

/// Associa a única bateria de um receptor ao único dispositivo encontrado nele.
pub fn pairing(found_ids: &[String], batteries: &[PathBuf]) -> Option<(String, PathBuf)> {
    match (found_ids, batteries) {
        ([id], [path]) => Some((id.clone(), path.clone())),
        _ => None,
    }
}

pub struct SysBackend {
    sysfs: PathBuf,
    dev: PathBuf,
    batteries: HashMap<String, PathBuf>,
}

impl SysBackend {
    pub fn new() -> Self {
        Self::with_roots(Path::new("/sys"), Path::new("/dev"))
    }

    pub fn with_roots(sysfs: &Path, dev: &Path) -> Self {
        SysBackend {
            sysfs: sysfs.to_path_buf(),
            dev: dev.to_path_buf(),
            batteries: HashMap::new(),
        }
    }
}

impl Default for SysBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn probe_slot(node: &HidNode, slot: u8) -> Option<Found> {
    let transport = HidrawTransport::open(&node.devnode).ok()?;
    let mut dev = Device::new(transport, slot);
    dev.set_timeout(PROBE_TIMEOUT);
    match dev.ping() {
        Ok((major, _)) if major >= 2 => {}
        _ => return None,
    }
    dev.set_timeout(DEFAULT_TIMEOUT);
    dev.discover().ok()?;
    let name = device_name::read(&mut dev).unwrap_or_else(|_| node.name.clone());
    let id = format!("{:04x}{:04x}-{}", node.vendor_id, node.product_id, slot);
    Some(Found {
        id,
        name,
        mouse: Box::new(HidppMouse::new(dev)),
    })
}

impl Backend for SysBackend {
    fn scan(&mut self) -> Vec<Found> {
        let nodes = match top_level_nodes(&self.sysfs, &self.dev) {
            Ok(nodes) => nodes,
            Err(e) => {
                eprintln!("ghubd: não foi possível listar os nós hidraw: {e}");
                return Vec::new();
            }
        };
        self.batteries.clear();
        let mut all = Vec::new();
        for node in nodes {
            if let Err(e) = HidrawTransport::open(&node.devnode) {
                eprintln!("ghubd: sem acesso a {} ({e}); veja docs/udev", node.devnode.display());
                continue;
            }
            let found: Vec<Found> = SLOTS.filter_map(|slot| probe_slot(&node, slot)).collect();
            let ids: Vec<String> = found.iter().map(|f| f.id.clone()).collect();
            if let Some((id, path)) = pairing(&ids, &find_batteries(&node.hid_dir)) {
                self.batteries.insert(id, path);
            }
            all.extend(found);
        }
        all
    }

    fn battery(&mut self, id: &str) -> Option<BatteryInfo> {
        self.batteries.get(id).and_then(|p| read_battery(p))
    }
}
```

- [ ] **Step 6: Rodar tudo**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos. (`SysBackend::scan` e `probe_slot` são verificados só por compilação; a Task 9 os exercita no hardware.)

- [ ] **Step 7: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): add Mouse trait, HID++ mouse and sysfs backend"
```

---

### Task 6: Gerente de dispositivos (`manager`)

**Files:**
- Create: `crates/ghubd/src/testing.rs` (somente testes)
- Create: `crates/ghubd/src/manager.rs`
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Consumes: Tasks 2 a 5 (`DeviceSummary`, `DeviceState`, `BATTERY_UNKNOWN`, `ProfileStore`, `Profile`, `dpi_allowed`, `ProfileError`, `Backend`, `Found`, `Mouse`, `Capabilities`, `BatteryInfo`, `ChargeStatus`).
- Produces (`ghubd::manager`):
  - `Event { DevicesChanged, DeviceChanged(String), ProfileChanged(String), BatteryLow { id: String, percent: u8 } }`.
  - `ManagerError { NotFound(String), Invalid(String), Offline(String), Device(String) }` (`Display`, `Error`; `From<ProfileError>`).
  - `Command` (enum com canais de resposta): `Devices(Sender<Vec<DeviceSummary>>)`, `State { id, reply }`, `SetDpiLevels { id, levels: Vec<u16>, active: usize, reply }`, `SetActiveLevel { id, active: usize, reply }`, `SetPolling { id, hz: u16, reply }`, `ListProfiles(Sender<Vec<String>>)`, `ActiveProfile(Sender<String>)`, `ActivateProfile { name, reply }`, `CreateProfile { name, reply }`, `DeleteProfile { name, reply }`, `Rescan(Option<Sender<()>>)`, `Shutdown`.
  - `Manager<B: Backend>`: `new(backend, store, store_path: PathBuf, events: Sender<Event>)`; métodos síncronos `devices()`, `state(&mut self, &str)`, `set_dpi_levels`, `set_active_level`, `set_polling`, `list_profiles`, `active_profile`, `activate_profile`, `create_profile`, `delete_profile`, `rescan`, `tick`, `handle(Command)`, `run(self, rx: Receiver<Command>, tick: Duration)`.
  - Constantes `LOW_BATTERY_PERCENT = 15`, `BATTERY_RECOVERED_PERCENT = 20`.
  - Em `testing` (só `cfg(test)`): `FakeMouse`, `FakeState`, `FakeBackend`.

- [ ] **Step 1: Criar os dublês de teste**

`crates/ghubd/src/testing.rs`:

```rust
//! Dublês compartilhados pelos testes do gerente e do serviço D-Bus.

use std::sync::{Arc, Mutex, MutexGuard};

use hidpp::features::dpi::DpiSupport;
use hidpp::HidppError;

use crate::backend::{Backend, Found};
use crate::battery_sysfs::BatteryInfo;
use crate::mouse::{Capabilities, Mouse, MouseResult};

pub struct FakeState {
    pub online: bool,
    pub dpi: u16,
    pub polling: u16,
    pub caps: Capabilities,
    pub dpi_sets: Vec<u16>,
    pub polling_sets: Vec<u16>,
}

#[derive(Clone)]
pub struct FakeMouse(pub Arc<Mutex<FakeState>>);

impl FakeMouse {
    pub fn new(dpi: u16, polling: u16) -> Self {
        FakeMouse(Arc::new(Mutex::new(FakeState {
            online: true,
            dpi,
            polling,
            caps: Capabilities {
                dpi: DpiSupport::Range { min: 100, max: 25600, step: 50 },
                polling_hz: vec![125, 250, 500, 1000],
            },
            dpi_sets: Vec::new(),
            polling_sets: Vec::new(),
        })))
    }

    pub fn state(&self) -> MutexGuard<'_, FakeState> {
        self.0.lock().unwrap()
    }
}

impl Mouse for FakeMouse {
    fn capabilities(&mut self) -> MouseResult<Capabilities> {
        Ok(self.state().caps.clone())
    }

    fn read_dpi(&mut self) -> MouseResult<u16> {
        let s = self.state();
        if s.online { Ok(s.dpi) } else { Err(HidppError::Timeout) }
    }

    fn read_polling_hz(&mut self) -> MouseResult<u16> {
        let s = self.state();
        if s.online { Ok(s.polling) } else { Err(HidppError::Timeout) }
    }

    fn set_dpi(&mut self, dpi: u16) -> MouseResult<u16> {
        let mut s = self.state();
        if !s.online {
            return Err(HidppError::Timeout);
        }
        s.dpi = dpi;
        s.dpi_sets.push(dpi);
        Ok(dpi)
    }

    fn set_polling_hz(&mut self, hz: u16) -> MouseResult<()> {
        let mut s = self.state();
        if !s.online {
            return Err(HidppError::Timeout);
        }
        s.polling = hz;
        s.polling_sets.push(hz);
        Ok(())
    }

    fn ping(&mut self) -> MouseResult<()> {
        if self.state().online { Ok(()) } else { Err(HidppError::Timeout) }
    }
}

pub struct FakeBackend {
    pub mice: Vec<(String, String, FakeMouse)>,
    pub battery: Arc<Mutex<Option<BatteryInfo>>>,
}

impl FakeBackend {
    /// Um backend com um único mouse `dev-1` ("G703 LS") e um handle para a bateria.
    pub fn with_mouse(mouse: FakeMouse) -> (Self, Arc<Mutex<Option<BatteryInfo>>>) {
        let battery = Arc::new(Mutex::new(None));
        (
            FakeBackend {
                mice: vec![("dev-1".to_string(), "G703 LS".to_string(), mouse)],
                battery: battery.clone(),
            },
            battery,
        )
    }
}

impl Backend for FakeBackend {
    fn scan(&mut self) -> Vec<Found> {
        self.mice
            .iter()
            .filter(|(_, _, m)| m.state().online)
            .map(|(id, name, m)| Found {
                id: id.clone(),
                name: name.clone(),
                mouse: Box::new(m.clone()),
            })
            .collect()
    }

    fn battery(&mut self, _id: &str) -> Option<BatteryInfo> {
        self.battery.lock().unwrap().clone()
    }
}
```

Em `lib.rs` acrescentar:

```rust
pub mod manager;
#[cfg(test)]
mod testing;
```

- [ ] **Step 2: Escrever os testes que falham**

`crates/ghubd/src/manager.rs` (somente `use` e testes; a implementação vem no Step 4):

```rust
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use ghub_ipc::{DeviceState, DeviceSummary, BATTERY_UNKNOWN, STATE_UNKNOWN};
use hidpp::features::dpi::DpiSupport;

use crate::backend::Backend;
use crate::battery_sysfs::{BatteryInfo, ChargeStatus};
use crate::mouse::{Capabilities, Mouse};
use crate::profile::{dpi_allowed, Profile, ProfileError, ProfileStore};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{FakeBackend, FakeMouse};
    use std::sync::mpsc::channel;
    use std::sync::{Arc, Mutex};

    type Handles = (
        Manager<FakeBackend>,
        Receiver<Event>,
        FakeMouse,
        Arc<Mutex<Option<BatteryInfo>>>,
        tempfile::TempDir,
    );

    fn setup_with(store: ProfileStore, dpi: u16, polling: u16) -> Handles {
        let dir = tempfile::tempdir().unwrap();
        let mouse = FakeMouse::new(dpi, polling);
        let (backend, battery) = FakeBackend::with_mouse(mouse.clone());
        let (tx, rx) = channel();
        let manager = Manager::new(backend, store, dir.path().join("profiles.toml"), tx);
        (manager, rx, mouse, battery, dir)
    }

    /// Armazenamento já "semeado": níveis 800 e 1600, ativo 1600, 1000 Hz.
    fn seeded_store() -> ProfileStore {
        ProfileStore::from_toml(
            "active = \"Padrão\"\n[[profile]]\nname = \"Padrão\"\ndpi_levels = [800, 1600]\nactive_level = 1\npolling_hz = 1000\n",
        )
        .unwrap()
    }

    fn drain(rx: &Receiver<Event>) -> Vec<Event> {
        rx.try_iter().collect()
    }

    fn battery(percent: u8, status: ChargeStatus) -> Option<BatteryInfo> {
        Some(BatteryInfo { percent, status, voltage_mv: Some(3900) })
    }

    #[test]
    fn first_run_seeds_profile_and_does_not_write_to_the_mouse() {
        let (mut m, rx, mouse, _b, dir) = setup_with(ProfileStore::default_store(), 1200, 1000);
        m.rescan();
        assert_eq!(m.store.active().dpi_levels, vec![1200]);
        assert_eq!(m.store.active().polling_hz, 1000);
        assert!(mouse.state().dpi_sets.is_empty());
        assert!(mouse.state().polling_sets.is_empty());
        let saved = std::fs::read_to_string(dir.path().join("profiles.toml")).unwrap();
        assert!(saved.contains("1200"));
        assert!(drain(&rx).contains(&Event::DevicesChanged));
    }

    #[test]
    fn rescan_applies_the_active_profile_when_it_differs() {
        let (mut m, rx, mouse, _b, _d) = setup_with(seeded_store(), 1200, 500);
        m.rescan();
        assert_eq!(mouse.state().dpi_sets, vec![1600]);
        assert_eq!(mouse.state().polling_sets, vec![1000]);
        let events = drain(&rx);
        assert!(events.contains(&Event::DeviceChanged("dev-1".into())));
        assert!(events.contains(&Event::DevicesChanged));
    }

    #[test]
    fn devices_and_state_report_what_the_device_says() {
        let (mut m, _rx, _mouse, battery_handle, _d) = setup_with(seeded_store(), 1600, 1000);
        *battery_handle.lock().unwrap() = battery(91, ChargeStatus::Discharging);
        m.rescan();
        m.tick();
        let list = m.devices();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "dev-1");
        assert_eq!(list[0].name, "G703 LS");
        assert!(list[0].connected);
        assert_eq!(list[0].battery_percent, 91);
        assert_eq!(list[0].battery_state, "discharging");
        let st = m.state("dev-1").unwrap();
        assert_eq!(st.dpi_current, 1600);
        assert_eq!(st.dpi_levels, vec![800, 1600]);
        assert_eq!(st.active_level, 1);
        assert_eq!((st.dpi_min, st.dpi_max, st.dpi_step), (100, 25600, 50));
        assert_eq!(st.polling_hz, 1000);
        assert_eq!(st.polling_supported, vec![125, 250, 500, 1000]);
        assert_eq!(st.profile, "Padrão");
    }

    #[test]
    fn unknown_device_is_not_found() {
        let (mut m, _rx, _mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        assert!(matches!(m.state("nope"), Err(ManagerError::NotFound(_))));
        assert!(matches!(
            m.set_active_level("nope", 0),
            Err(ManagerError::NotFound(_))
        ));
    }

    #[test]
    fn set_dpi_levels_validates_before_touching_the_device() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        let before = mouse.state().dpi_sets.len();
        // 125 não está na grade de 50 a partir de 100; lista vazia; excesso; nível ativo fora.
        for (levels, active) in [
            (vec![125u16], 0usize),
            (vec![], 0),
            (vec![200, 300, 400, 500, 600, 700], 0),
            (vec![400, 800], 2),
            (vec![50], 0),
        ] {
            assert!(
                matches!(m.set_dpi_levels("dev-1", levels.clone(), active), Err(ManagerError::Invalid(_))),
                "{levels:?}/{active}"
            );
        }
        assert_eq!(mouse.state().dpi_sets.len(), before);
        assert_eq!(m.store.active().dpi_levels, vec![800, 1600]);
    }

    #[test]
    fn set_dpi_levels_applies_persists_and_notifies() {
        let (mut m, rx, mouse, _b, dir) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        drain(&rx);
        m.set_dpi_levels("dev-1", vec![400, 3200], 1).unwrap();
        assert_eq!(mouse.state().dpi, 3200);
        assert_eq!(m.store.active().dpi_levels, vec![400, 3200]);
        let saved = std::fs::read_to_string(dir.path().join("profiles.toml")).unwrap();
        assert!(saved.contains("3200"));
        let events = drain(&rx);
        assert!(events.contains(&Event::ProfileChanged("Padrão".into())));
        assert!(events.contains(&Event::DeviceChanged("dev-1".into())));
    }

    #[test]
    fn set_active_level_switches_the_dpi() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        m.set_active_level("dev-1", 0).unwrap();
        assert_eq!(mouse.state().dpi, 800);
        assert!(matches!(
            m.set_active_level("dev-1", 5),
            Err(ManagerError::Invalid(_))
        ));
    }

    #[test]
    fn set_polling_validates_against_the_device() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        assert!(matches!(m.set_polling("dev-1", 333), Err(ManagerError::Invalid(_))));
        assert!(matches!(m.set_polling("dev-1", 0), Err(ManagerError::Invalid(_))));
        m.set_polling("dev-1", 500).unwrap();
        assert_eq!(mouse.state().polling, 500);
        assert_eq!(m.store.active().polling_hz, 500);
    }

    #[test]
    fn offline_device_rejects_writes() {
        let (mut m, _rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        mouse.state().online = false;
        m.tick();
        assert!(matches!(
            m.set_dpi_levels("dev-1", vec![800], 0),
            Err(ManagerError::Offline(_))
        ));
        let st = m.state("dev-1").unwrap();
        assert!(!st.connected);
        assert_eq!(st.dpi_current, 0);
        assert_eq!(st.dpi_levels, vec![800, 1600]);
    }

    #[test]
    fn reconnect_reapplies_the_profile() {
        let (mut m, rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        drain(&rx);
        mouse.state().online = false;
        m.tick();
        assert!(!m.devices()[0].connected);
        assert!(drain(&rx).contains(&Event::DeviceChanged("dev-1".into())));
        // O mouse volta com as configurações de fábrica.
        {
            let mut s = mouse.state();
            s.online = true;
            s.dpi = 800;
            s.polling = 125;
        }
        m.tick();
        assert!(m.devices()[0].connected);
        assert_eq!(mouse.state().dpi, 1600);
        assert_eq!(mouse.state().polling, 1000);
    }

    #[test]
    fn battery_low_is_reported_once_until_recovery() {
        let (mut m, rx, _mouse, bat, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        drain(&rx);
        let lows = |rx: &Receiver<Event>| {
            drain(rx)
                .into_iter()
                .filter(|e| matches!(e, Event::BatteryLow { .. }))
                .count()
        };
        *bat.lock().unwrap() = battery(14, ChargeStatus::Discharging);
        m.tick();
        assert_eq!(lows(&rx), 1);
        m.tick();
        assert_eq!(lows(&rx), 0);
        *bat.lock().unwrap() = battery(25, ChargeStatus::Discharging);
        m.tick();
        assert_eq!(lows(&rx), 0);
        *bat.lock().unwrap() = battery(10, ChargeStatus::Discharging);
        m.tick();
        assert_eq!(lows(&rx), 1);
    }

    #[test]
    fn charging_battery_never_reports_low() {
        let (mut m, rx, _mouse, bat, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        drain(&rx);
        *bat.lock().unwrap() = battery(5, ChargeStatus::Charging);
        m.tick();
        assert!(!drain(&rx).iter().any(|e| matches!(e, Event::BatteryLow { .. })));
    }

    #[test]
    fn profile_lifecycle_through_the_manager() {
        let (mut m, rx, mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        m.rescan();
        m.create_profile("Trabalho").unwrap();
        assert!(matches!(m.create_profile("Trabalho"), Err(ManagerError::Invalid(_))));
        assert_eq!(m.list_profiles(), vec!["Padrão".to_string(), "Trabalho".to_string()]);
        m.set_dpi_levels("dev-1", vec![400, 800], 0).unwrap(); // altera o ativo (Padrão)
        drain(&rx);
        m.activate_profile("Trabalho").unwrap(); // cópia do Padrão no momento da criação: 800/1600, ativo 1600
        assert_eq!(m.active_profile(), "Trabalho");
        assert_eq!(mouse.state().dpi, 1600);
        assert!(drain(&rx).contains(&Event::ProfileChanged("Trabalho".into())));
        assert!(matches!(m.activate_profile("zzz"), Err(ManagerError::NotFound(_))));
        assert!(matches!(m.delete_profile("Trabalho"), Err(ManagerError::Invalid(_))));
        m.delete_profile("Padrão").unwrap();
    }

    #[test]
    fn handle_dispatches_commands_to_their_reply_channels() {
        let (mut m, _rx, _mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        let (tx, rx) = channel();
        m.handle(Command::Rescan(Some(tx)));
        rx.recv().unwrap();
        let (tx, rx) = channel();
        m.handle(Command::Devices(tx));
        assert_eq!(rx.recv().unwrap().len(), 1);
        let (tx, rx) = channel();
        m.handle(Command::ActiveProfile(tx));
        assert_eq!(rx.recv().unwrap(), "Padrão");
        let (tx, rx) = channel();
        m.handle(Command::State { id: "nope".into(), reply: tx });
        assert!(matches!(rx.recv().unwrap(), Err(ManagerError::NotFound(_))));
    }

    #[test]
    fn run_stops_on_shutdown() {
        let (m, _rx, _mouse, _b, _d) = setup_with(seeded_store(), 1600, 1000);
        let (tx, crx) = channel();
        tx.send(Command::Shutdown).unwrap();
        m.run(crx, Duration::from_millis(10)); // retorna em vez de travar
    }
}
```

- [ ] **Step 3: Rodar e ver falhar**

Run: `cargo test -p ghubd manager`
Expected: FAIL de compilação (`Manager`, `Event`, `Command`, `ManagerError` não definidos).

- [ ] **Step 4: Implementar**

Inserir em `manager.rs`, entre os `use` e o bloco de testes:

```rust
pub const LOW_BATTERY_PERCENT: u8 = 15;
pub const BATTERY_RECOVERED_PERCENT: u8 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    DevicesChanged,
    DeviceChanged(String),
    ProfileChanged(String),
    BatteryLow { id: String, percent: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagerError {
    NotFound(String),
    Invalid(String),
    Offline(String),
    Device(String),
}

impl std::fmt::Display for ManagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ManagerError::NotFound(what) => write!(f, "não encontrado: {what}"),
            ManagerError::Invalid(why) => write!(f, "{why}"),
            ManagerError::Offline(id) => write!(f, "o dispositivo {id} está offline"),
            ManagerError::Device(why) => write!(f, "erro no dispositivo: {why}"),
        }
    }
}

impl std::error::Error for ManagerError {}

impl From<ProfileError> for ManagerError {
    fn from(e: ProfileError) -> Self {
        match e {
            ProfileError::NotFound(n) => ManagerError::NotFound(n),
            other => ManagerError::Invalid(other.to_string()),
        }
    }
}

pub enum Command {
    Devices(Sender<Vec<DeviceSummary>>),
    State { id: String, reply: Sender<Result<DeviceState, ManagerError>> },
    SetDpiLevels { id: String, levels: Vec<u16>, active: usize, reply: Sender<Result<(), ManagerError>> },
    SetActiveLevel { id: String, active: usize, reply: Sender<Result<(), ManagerError>> },
    SetPolling { id: String, hz: u16, reply: Sender<Result<(), ManagerError>> },
    ListProfiles(Sender<Vec<String>>),
    ActiveProfile(Sender<String>),
    ActivateProfile { name: String, reply: Sender<Result<(), ManagerError>> },
    CreateProfile { name: String, reply: Sender<Result<(), ManagerError>> },
    DeleteProfile { name: String, reply: Sender<Result<(), ManagerError>> },
    /// `None` quando o pedido vem do monitor udev (ninguém espera resposta).
    Rescan(Option<Sender<()>>),
    Shutdown,
}

struct Managed {
    id: String,
    name: String,
    /// `None` quando o dispositivo está offline.
    mouse: Option<Box<dyn Mouse>>,
    caps: Option<Capabilities>,
    battery: Option<BatteryInfo>,
    low_notified: bool,
}

pub struct Manager<B: Backend> {
    backend: B,
    store: ProfileStore,
    store_path: PathBuf,
    events: Sender<Event>,
    devices: Vec<Managed>,
}

fn battery_fields(b: &Option<BatteryInfo>) -> (i32, String) {
    match b {
        Some(b) => (i32::from(b.percent), b.status.as_str().to_string()),
        None => (BATTERY_UNKNOWN, STATE_UNKNOWN.to_string()),
    }
}

impl<B: Backend> Manager<B> {
    pub fn new(backend: B, store: ProfileStore, store_path: PathBuf, events: Sender<Event>) -> Self {
        Manager { backend, store, store_path, events, devices: Vec::new() }
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    fn persist(&self) {
        if let Err(e) = self.store.save(&self.store_path) {
            eprintln!("ghubd: não foi possível salvar {}: {e}", self.store_path.display());
        }
    }

    fn find(&self, id: &str) -> Result<usize, ManagerError> {
        self.devices
            .iter()
            .position(|d| d.id == id)
            .ok_or_else(|| ManagerError::NotFound(id.to_string()))
    }

    fn online_caps(&self, idx: usize) -> Result<Capabilities, ManagerError> {
        let d = &self.devices[idx];
        match (&d.mouse, &d.caps) {
            (Some(_), Some(caps)) => Ok(caps.clone()),
            _ => Err(ManagerError::Offline(d.id.clone())),
        }
    }

    // ----- consultas -----

    pub fn devices(&self) -> Vec<DeviceSummary> {
        self.devices
            .iter()
            .map(|d| {
                let (battery_percent, battery_state) = battery_fields(&d.battery);
                DeviceSummary {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    connected: d.mouse.is_some(),
                    battery_percent,
                    battery_state,
                }
            })
            .collect()
    }

    pub fn state(&mut self, id: &str) -> Result<DeviceState, ManagerError> {
        let idx = self.find(id)?;
        let profile = self.store.active().clone();
        let d = &mut self.devices[idx];
        let (dpi_current, polling_hz) = match d.mouse.as_mut() {
            Some(m) => (
                u32::from(m.read_dpi().unwrap_or(0)),
                u32::from(m.read_polling_hz().unwrap_or(0)),
            ),
            None => (0, 0),
        };
        let (dpi_min, dpi_max, dpi_step, dpi_list, polling_supported) = match &d.caps {
            Some(c) => {
                let (min, max, step, list) = match &c.dpi {
                    DpiSupport::Range { min, max, step } => {
                        (u32::from(*min), u32::from(*max), u32::from(*step), Vec::new())
                    }
                    DpiSupport::List(values) => (
                        values.first().copied().map(u32::from).unwrap_or(0),
                        values.last().copied().map(u32::from).unwrap_or(0),
                        0,
                        values.iter().copied().map(u32::from).collect(),
                    ),
                };
                (min, max, step, list, c.polling_hz.iter().copied().map(u32::from).collect())
            }
            None => (0, 0, 0, Vec::new(), Vec::new()),
        };
        let (battery_percent, battery_state) = battery_fields(&d.battery);
        Ok(DeviceState {
            id: d.id.clone(),
            name: d.name.clone(),
            connected: d.mouse.is_some(),
            dpi_current,
            dpi_levels: profile.dpi_levels.iter().copied().map(u32::from).collect(),
            active_level: profile.active_level as u32,
            dpi_min,
            dpi_max,
            dpi_step,
            dpi_list,
            polling_hz,
            polling_supported,
            battery_percent,
            battery_state,
            profile: profile.name,
        })
    }

    pub fn list_profiles(&self) -> Vec<String> {
        self.store.names()
    }

    pub fn active_profile(&self) -> String {
        self.store.active_name().to_string()
    }

    // ----- mudanças -----

    fn commit_profile_change(&mut self, idx: usize, candidate: Profile) {
        self.store.update_active(candidate);
        self.persist();
        self.apply_to(idx);
        let name = self.store.active_name().to_string();
        let id = self.devices[idx].id.clone();
        self.emit(Event::ProfileChanged(name));
        self.emit(Event::DeviceChanged(id));
    }

    pub fn set_dpi_levels(&mut self, id: &str, levels: Vec<u16>, active: usize) -> Result<(), ManagerError> {
        let idx = self.find(id)?;
        let caps = self.online_caps(idx)?;
        let mut candidate = self.store.active().clone();
        candidate.dpi_levels = levels;
        candidate.active_level = active;
        candidate.check_shape()?;
        for dpi in &candidate.dpi_levels {
            if !dpi_allowed(&caps.dpi, *dpi) {
                return Err(ManagerError::Invalid(format!(
                    "DPI {dpi} não é suportado por este dispositivo"
                )));
            }
        }
        self.commit_profile_change(idx, candidate);
        Ok(())
    }

    pub fn set_active_level(&mut self, id: &str, active: usize) -> Result<(), ManagerError> {
        let idx = self.find(id)?;
        self.online_caps(idx)?;
        let mut candidate = self.store.active().clone();
        candidate.active_level = active;
        candidate.check_shape()?;
        self.commit_profile_change(idx, candidate);
        Ok(())
    }

    pub fn set_polling(&mut self, id: &str, hz: u16) -> Result<(), ManagerError> {
        let idx = self.find(id)?;
        let caps = self.online_caps(idx)?;
        if !caps.polling_hz.contains(&hz) {
            return Err(ManagerError::Invalid(format!(
                "taxa de polling {hz} Hz não é suportada por este dispositivo"
            )));
        }
        let mut candidate = self.store.active().clone();
        candidate.polling_hz = hz;
        candidate.check_shape()?;
        self.commit_profile_change(idx, candidate);
        Ok(())
    }

    pub fn activate_profile(&mut self, name: &str) -> Result<(), ManagerError> {
        self.store.activate(name)?;
        self.persist();
        for idx in 0..self.devices.len() {
            self.apply_to(idx);
        }
        self.emit(Event::ProfileChanged(name.to_string()));
        for id in self.devices.iter().filter(|d| d.mouse.is_some()).map(|d| d.id.clone()).collect::<Vec<_>>() {
            self.emit(Event::DeviceChanged(id));
        }
        Ok(())
    }

    pub fn create_profile(&mut self, name: &str) -> Result<(), ManagerError> {
        self.store.create(name)?;
        self.persist();
        self.emit(Event::ProfileChanged(self.store.active_name().to_string()));
        Ok(())
    }

    pub fn delete_profile(&mut self, name: &str) -> Result<(), ManagerError> {
        self.store.delete(name)?;
        self.persist();
        self.emit(Event::ProfileChanged(self.store.active_name().to_string()));
        Ok(())
    }

    /// Aplica o perfil ativo ao dispositivo, escrevendo apenas o que difere do estado atual
    /// e apenas valores que o dispositivo anunciou como suportados.
    fn apply_to(&mut self, idx: usize) {
        let profile = self.store.active().clone();
        let d = &mut self.devices[idx];
        let (Some(mouse), Some(caps)) = (d.mouse.as_mut(), d.caps.as_ref()) else {
            return;
        };
        if caps.polling_hz.contains(&profile.polling_hz) {
            if mouse.read_polling_hz().ok() != Some(profile.polling_hz) {
                if let Err(e) = mouse.set_polling_hz(profile.polling_hz) {
                    eprintln!("ghubd: falha ao definir o polling de {}: {e}", d.id);
                }
            }
        } else {
            eprintln!("ghubd: {} não suporta {} Hz; ignorado", d.id, profile.polling_hz);
        }
        let dpi = profile.active_dpi();
        if dpi_allowed(&caps.dpi, dpi) {
            if mouse.read_dpi().ok() != Some(dpi) {
                if let Err(e) = mouse.set_dpi(dpi) {
                    eprintln!("ghubd: falha ao definir o DPI de {}: {e}", d.id);
                }
            }
        } else {
            eprintln!("ghubd: {} não suporta {dpi} DPI; ignorado", d.id);
        }
    }

    // ----- ciclo de vida dos dispositivos -----

    /// Lê as capacidades, semeia o perfil na primeira execução e aplica o perfil ativo.
    fn bring_online(&mut self, idx: usize) {
        let probe = {
            let Some(mouse) = self.devices[idx].mouse.as_mut() else {
                return;
            };
            (mouse.capabilities(), mouse.read_dpi(), mouse.read_polling_hz())
        };
        let (caps, dpi, hz) = probe;
        match caps {
            Ok(c) => self.devices[idx].caps = Some(c),
            Err(e) => {
                eprintln!("ghubd: {}: não foi possível ler as capacidades: {e}", self.devices[idx].id);
                self.devices[idx].mouse = None;
                return;
            }
        }
        if self.store.needs_seed() {
            if let (Ok(dpi), Ok(hz)) = (dpi, hz) {
                if dpi > 0 && hz > 0 {
                    self.store.seed(dpi, hz);
                    self.persist();
                }
            }
        }
        self.apply_to(idx);
        let id = self.devices[idx].id.clone();
        self.emit(Event::DeviceChanged(id));
    }

    pub fn rescan(&mut self) {
        let found = self.backend.scan();
        let mut changed = false;
        for f in found {
            let idx = match self.devices.iter().position(|d| d.id == f.id) {
                Some(i) if self.devices[i].mouse.is_some() => continue,
                Some(i) => {
                    self.devices[i].name = f.name;
                    self.devices[i].mouse = Some(f.mouse);
                    i
                }
                None => {
                    self.devices.push(Managed {
                        id: f.id,
                        name: f.name,
                        mouse: Some(f.mouse),
                        caps: None,
                        battery: None,
                        low_notified: false,
                    });
                    self.devices.len() - 1
                }
            };
            self.bring_online(idx);
            changed = true;
        }
        if changed {
            self.emit(Event::DevicesChanged);
        }
    }

    fn refresh_batteries(&mut self) {
        for idx in 0..self.devices.len() {
            let id = self.devices[idx].id.clone();
            let new = self.backend.battery(&id);
            let d = &mut self.devices[idx];
            let changed = d.battery != new;
            d.battery = new.clone();
            let mut events = Vec::new();
            if let Some(b) = &new {
                let discharging = !matches!(b.status, ChargeStatus::Charging | ChargeStatus::Full);
                if b.percent <= LOW_BATTERY_PERCENT && discharging && !d.low_notified {
                    d.low_notified = true;
                    events.push(Event::BatteryLow { id: id.clone(), percent: b.percent });
                } else if b.percent >= BATTERY_RECOVERED_PERCENT || !discharging {
                    d.low_notified = false;
                }
            }
            if changed {
                events.push(Event::DeviceChanged(id));
            }
            for e in events {
                self.emit(e);
            }
        }
    }

    /// Ciclo periódico: detecta dispositivos que sumiram, procura os que voltaram e atualiza a bateria.
    pub fn tick(&mut self) {
        let mut went_offline = Vec::new();
        for d in &mut self.devices {
            let alive = match d.mouse.as_mut() {
                Some(m) => m.ping().is_ok(),
                None => continue,
            };
            if !alive {
                d.mouse = None;
                d.caps = None;
                went_offline.push(d.id.clone());
            }
        }
        if !went_offline.is_empty() {
            for id in went_offline {
                self.emit(Event::DeviceChanged(id));
            }
            self.emit(Event::DevicesChanged);
        }
        if self.devices.is_empty() || self.devices.iter().any(|d| d.mouse.is_none()) {
            self.rescan();
        }
        self.refresh_batteries();
    }

    pub fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Devices(r) => {
                let _ = r.send(self.devices());
            }
            Command::State { id, reply } => {
                let _ = reply.send(self.state(&id));
            }
            Command::SetDpiLevels { id, levels, active, reply } => {
                let _ = reply.send(self.set_dpi_levels(&id, levels, active));
            }
            Command::SetActiveLevel { id, active, reply } => {
                let _ = reply.send(self.set_active_level(&id, active));
            }
            Command::SetPolling { id, hz, reply } => {
                let _ = reply.send(self.set_polling(&id, hz));
            }
            Command::ListProfiles(r) => {
                let _ = r.send(self.list_profiles());
            }
            Command::ActiveProfile(r) => {
                let _ = r.send(self.active_profile());
            }
            Command::ActivateProfile { name, reply } => {
                let _ = reply.send(self.activate_profile(&name));
            }
            Command::CreateProfile { name, reply } => {
                let _ = reply.send(self.create_profile(&name));
            }
            Command::DeleteProfile { name, reply } => {
                let _ = reply.send(self.delete_profile(&name));
            }
            Command::Rescan(reply) => {
                self.rescan();
                if let Some(r) = reply {
                    let _ = r.send(());
                }
            }
            Command::Shutdown => {}
        }
    }

    /// Laço principal: atende comandos e executa `tick` quando fica `tick` sem comandos.
    pub fn run(mut self, commands: Receiver<Command>, tick: Duration) {
        self.rescan();
        loop {
            match commands.recv_timeout(tick) {
                Ok(Command::Shutdown) => break,
                Ok(cmd) => self.handle(cmd),
                Err(RecvTimeoutError::Timeout) => self.tick(),
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }
}
```

Observação: se `cargo` acusar importação não usada em `manager.rs` (por exemplo `Mouse`), remover só o `use` excedente; nenhum teste muda.

- [ ] **Step 5: Rodar tudo**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos. Se algum teste de `manager` falhar por detalhe de ordem de eventos, corrigir o **código** conforme o comportamento descrito nos nomes dos testes, nunca enfraquecer um teste.

- [ ] **Step 6: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): add device manager with profile apply, reconnect and battery events"
```

---

### Task 7: Serviço D-Bus

**Files:**
- Create: `crates/ghubd/src/service.rs`
- Modify: `crates/ghubd/src/lib.rs`

**Interfaces:**
- Consumes: `manager::{Command, ManagerError}`, `ghub_ipc::{DeviceState, DeviceSummary}`, `FakeBackend`/`FakeMouse` (testes).
- Produces: `service::DaemonService::new(tx: Sender<Command>)`, objeto D-Bus `org.ghublinux.Daemon1` com os métodos `Devices`, `GetDeviceState`, `SetDpiLevels`, `SetActiveDpiLevel`, `SetPollingRate`, `ListProfiles`, `GetActiveProfile`, `ActivateProfile`, `CreateProfile`, `DeleteProfile`, `Rescan` (nomes derivados dos métodos Rust em `snake_case`), e `REPLY_TIMEOUT = 5 s`. Mapeamento de erros: `Invalid` → `InvalidArgs`; `NotFound` → `UnknownObject`; `Offline`/`Device`/gerente parado ou lento → `Failed`.

- [ ] **Step 1: Escrever os testes que falham**

`crates/ghubd/src/service.rs` (somente `use` e testes):

```rust
use std::sync::mpsc::{channel, Sender};
use std::time::Duration;

use ghub_ipc::{DeviceState, DeviceSummary};
use zbus::{fdo, interface};

use crate::manager::{Command, ManagerError};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::Manager;
    use crate::profile::ProfileStore;
    use crate::testing::{FakeBackend, FakeMouse};
    use std::thread;

    fn running_service() -> (DaemonService, Sender<Command>, thread::JoinHandle<()>, tempfile::TempDir) {
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
        (DaemonService::new(tx.clone()), tx, handle, dir)
    }

    fn stop(tx: Sender<Command>, handle: thread::JoinHandle<()>) {
        tx.send(Command::Shutdown).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn lists_devices_and_state() {
        let (svc, tx, handle, _d) = running_service();
        let devices = svc.devices().unwrap();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, "dev-1");
        let state = svc.get_device_state("dev-1").unwrap();
        assert_eq!(state.dpi_current, 1600);
        assert_eq!(svc.get_active_profile().unwrap(), "Padrão");
        assert_eq!(svc.list_profiles().unwrap(), vec!["Padrão".to_string()]);
        stop(tx, handle);
    }

    #[test]
    fn good_writes_succeed_and_bad_input_is_invalid_args() {
        let (svc, tx, handle, _d) = running_service();
        svc.set_dpi_levels("dev-1", vec![400, 3200], 1).unwrap();
        assert_eq!(svc.get_device_state("dev-1").unwrap().dpi_current, 3200);
        svc.set_active_dpi_level("dev-1", 0).unwrap();
        svc.set_polling_rate("dev-1", 500).unwrap();

        let invalid = |r: fdo::Result<()>| matches!(r, Err(fdo::Error::InvalidArgs(_)));
        assert!(invalid(svc.set_dpi_levels("dev-1", vec![125], 0))); // fora da grade
        assert!(invalid(svc.set_dpi_levels("dev-1", vec![70000], 0))); // não cabe em u16
        assert!(invalid(svc.set_dpi_levels("dev-1", vec![], 0)));
        assert!(invalid(svc.set_dpi_levels("dev-1", vec![400], 9)));
        assert!(invalid(svc.set_active_dpi_level("dev-1", 7)));
        assert!(invalid(svc.set_polling_rate("dev-1", 333)));
        assert!(invalid(svc.set_polling_rate("dev-1", 70000)));
        assert!(invalid(svc.create_profile("")));
        assert!(invalid(svc.create_profile("a\nb")));
        // Nada disso mexeu no mouse.
        assert_eq!(svc.get_device_state("dev-1").unwrap().dpi_current, 400);
        stop(tx, handle);
    }

    #[test]
    fn unknown_ids_and_profiles_map_to_unknown_object() {
        let (svc, tx, handle, _d) = running_service();
        assert!(matches!(svc.get_device_state("nope"), Err(fdo::Error::UnknownObject(_))));
        assert!(matches!(svc.activate_profile("zzz"), Err(fdo::Error::UnknownObject(_))));
        assert!(matches!(svc.delete_profile("zzz"), Err(fdo::Error::UnknownObject(_))));
        stop(tx, handle);
    }

    #[test]
    fn profiles_can_be_created_activated_and_deleted() {
        let (svc, tx, handle, _d) = running_service();
        svc.create_profile("Trabalho").unwrap();
        svc.activate_profile("Trabalho").unwrap();
        assert_eq!(svc.get_active_profile().unwrap(), "Trabalho");
        svc.activate_profile("Padrão").unwrap();
        svc.delete_profile("Trabalho").unwrap();
        svc.rescan().unwrap();
        stop(tx, handle);
    }

    #[test]
    fn stopped_manager_gives_failed_not_a_hang() {
        let (svc, tx, handle, _d) = running_service();
        stop(tx, handle);
        assert!(matches!(svc.devices(), Err(fdo::Error::Failed(_))));
    }

    #[test]
    fn slow_manager_times_out_with_failed() {
        // Ninguém atende o canal: o serviço deve desistir em vez de travar o cliente.
        let (tx, _rx_kept_alive) = channel::<Command>();
        let svc = DaemonService::with_timeout(tx, Duration::from_millis(50));
        assert!(matches!(svc.devices(), Err(fdo::Error::Failed(_))));
    }
}
```

- [ ] **Step 2: Rodar e ver falhar**

Em `lib.rs` acrescentar `pub mod service;`.

Run: `cargo test -p ghubd service`
Expected: FAIL de compilação (`DaemonService` não definido).

- [ ] **Step 3: Implementar**

Inserir em `service.rs`, entre os `use` e os testes:

```rust
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

fn to_fdo(e: ManagerError) -> fdo::Error {
    match e {
        ManagerError::Invalid(m) => fdo::Error::InvalidArgs(m),
        ManagerError::NotFound(m) => fdo::Error::UnknownObject(m),
        ManagerError::Offline(_) | ManagerError::Device(_) => fdo::Error::Failed(e.to_string()),
    }
}

fn to_u16(value: u32) -> fdo::Result<u16> {
    u16::try_from(value)
        .map_err(|_| fdo::Error::InvalidArgs(format!("valor fora do intervalo: {value}")))
}

/// Ponte entre o D-Bus e o gerente: cada chamada vira um `Command` e espera a resposta.
pub struct DaemonService {
    tx: Sender<Command>,
    timeout: Duration,
}

impl DaemonService {
    pub fn new(tx: Sender<Command>) -> Self {
        Self::with_timeout(tx, REPLY_TIMEOUT)
    }

    pub fn with_timeout(tx: Sender<Command>, timeout: Duration) -> Self {
        DaemonService { tx, timeout }
    }

    fn ask<T>(&self, make: impl FnOnce(Sender<T>) -> Command) -> fdo::Result<T> {
        let (reply_tx, reply_rx) = channel();
        self.tx
            .send(make(reply_tx))
            .map_err(|_| fdo::Error::Failed("o gerente de dispositivos parou".into()))?;
        reply_rx
            .recv_timeout(self.timeout)
            .map_err(|_| fdo::Error::Failed("o gerente de dispositivos não respondeu a tempo".into()))
    }

    fn ask_result<T>(
        &self,
        make: impl FnOnce(Sender<Result<T, ManagerError>>) -> Command,
    ) -> fdo::Result<T> {
        self.ask(make)?.map_err(to_fdo)
    }
}

#[interface(name = "org.ghublinux.Daemon1")]
impl DaemonService {
    fn devices(&self) -> fdo::Result<Vec<DeviceSummary>> {
        self.ask(Command::Devices)
    }

    fn get_device_state(&self, id: &str) -> fdo::Result<DeviceState> {
        let id = id.to_string();
        self.ask_result(|reply| Command::State { id, reply })
    }

    fn set_dpi_levels(&self, id: &str, levels: Vec<u32>, active: u32) -> fdo::Result<()> {
        let levels = levels.into_iter().map(to_u16).collect::<fdo::Result<Vec<u16>>>()?;
        let id = id.to_string();
        self.ask_result(|reply| Command::SetDpiLevels { id, levels, active: active as usize, reply })
    }

    fn set_active_dpi_level(&self, id: &str, active: u32) -> fdo::Result<()> {
        let id = id.to_string();
        self.ask_result(|reply| Command::SetActiveLevel { id, active: active as usize, reply })
    }

    fn set_polling_rate(&self, id: &str, hz: u32) -> fdo::Result<()> {
        let hz = to_u16(hz)?;
        let id = id.to_string();
        self.ask_result(|reply| Command::SetPolling { id, hz, reply })
    }

    fn list_profiles(&self) -> fdo::Result<Vec<String>> {
        self.ask(Command::ListProfiles)
    }

    fn get_active_profile(&self) -> fdo::Result<String> {
        self.ask(Command::ActiveProfile)
    }

    fn activate_profile(&self, name: &str) -> fdo::Result<()> {
        let name = name.to_string();
        self.ask_result(|reply| Command::ActivateProfile { name, reply })
    }

    fn create_profile(&self, name: &str) -> fdo::Result<()> {
        let name = name.to_string();
        self.ask_result(|reply| Command::CreateProfile { name, reply })
    }

    fn delete_profile(&self, name: &str) -> fdo::Result<()> {
        let name = name.to_string();
        self.ask_result(|reply| Command::DeleteProfile { name, reply })
    }

    fn rescan(&self) -> fdo::Result<()> {
        self.ask(|reply| Command::Rescan(Some(reply)))
    }
}
```

- [ ] **Step 4: Rodar tudo**

Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS, sem avisos. (Os métodos são chamados diretamente como funções Rust; o barramento D-Bus não é usado nos testes.)

- [ ] **Step 5: Commit**

```bash
git add crates/ghubd
git commit -m "feat(ghubd): add D-Bus service bridging to the device manager"
```

---

### Task 8: Monitor udev, `main` e pacote systemd

**Files:**
- Create: `crates/ghubd/src/watcher.rs`
- Modify: `crates/ghubd/src/lib.rs`
- Modify: `crates/ghubd/src/main.rs`
- Create: `packaging/systemd/ghubd.service`
- Create: `packaging/install-user.sh`

**Interfaces:**
- Consumes: `manager::{Command, Event, Manager}`, `service::DaemonService`, `profile::{store_path_from, ProfileStore}`, `backend::SysBackend`, `ghub_ipc::{BUS_NAME, OBJECT_PATH, INTERFACE}`.
- Produces: `watcher::spawn(tx: Sender<Command>)` (thread que envia `Command::Rescan(None)` quando há eventos `hidraw`, com debounce de 500 ms); binário `ghubd` que sobe o barramento, o gerente (ciclo de 5 s), o emissor de sinais e o monitor; `packaging/systemd/ghubd.service` (`Type=dbus`).

- [ ] **Step 1: Monitor udev**

`crates/ghubd/src/watcher.rs`:

```rust
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
```

Em `lib.rs` acrescentar `pub mod watcher;`.

- [ ] **Step 2: `main.rs`**

`crates/ghubd/src/main.rs`:

```rust
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
```

- [ ] **Step 3: Compilar e testar**

Run: `RUSTFLAGS="-D warnings" cargo build --workspace && RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: compila sem avisos; todos os testes passam.

- [ ] **Step 4: Pacote systemd**

`packaging/systemd/ghubd.service`:

```ini
[Unit]
Description=ghub-linux daemon (dispositivos Logitech)

[Service]
Type=dbus
BusName=org.ghublinux.Daemon
ExecStart=%h/.local/bin/ghubd
Restart=on-failure
RestartSec=2

[Install]
WantedBy=default.target
```

`packaging/install-user.sh`:

```bash
#!/usr/bin/env bash
# Instala o ghubd para o usuário atual (sem sudo). Não habilita o serviço.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cargo build --release -p ghubd --manifest-path "$root/Cargo.toml"

install -Dm755 "$root/target/release/ghubd" "$HOME/.local/bin/ghubd"
install -Dm644 "$root/packaging/systemd/ghubd.service" "$HOME/.config/systemd/user/ghubd.service"
systemctl --user daemon-reload

echo "Instalado. Para iniciar agora:        systemctl --user start ghubd"
echo "Para iniciar com a sessão (opcional): systemctl --user enable ghubd"
```

Run: `chmod +x packaging/install-user.sh && bash -n packaging/install-user.sh`
Expected: sem saída (sintaxe válida). **Não executar o script nesta task.**

- [ ] **Step 5: Commit**

```bash
git add crates/ghubd packaging
git commit -m "feat(ghubd): add udev watcher, daemon entry point and systemd user unit"
```

---

### Task 9: Verificação no hardware e documentação

**Files:**
- Create: `docs/daemon.md`
- Create: `docs/hardware/daemon-verification.md`
- Modify: `docs/superpowers/specs/2026-10-12-ghub-linux-design.md` (seções 5.2 e 5.3)
- Modify: `README.md`

**Interfaces:**
- Consumes: binário `ghubd` (Task 8) e o G703 real.

Esta task usa o mouse de verdade. **O controlador pede autorização ao usuário antes dos Steps 3 e 5**, pois eles escrevem no dispositivo (DPI e polling; reversíveis pelo botão de DPI e pelo perfil). Nenhum passo usa `sudo`.

- [ ] **Step 1: Subir o daemon em primeiro plano (somente leitura)**

Mover o mouse antes. Em segundo plano:

```bash
cargo run -q -p ghubd 2>/tmp/ghubd.log
```

Em outro terminal, conferir que o nome foi obtido e que nada foi escrito ainda:

```bash
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 Devices
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 GetDeviceState s 046dc539-1
cat ~/.config/ghub-linux/profiles.toml
```

Expected: `Devices` lista um dispositivo `046dc539-1` conectado, com `battery_percent` igual ao `capacity` de `/sys/class/power_supply/hidpp_battery_0/capacity`; `GetDeviceState` traz `dpi_current` igual ao DPI real do mouse (antes da execução), `dpi_min=100`, `dpi_max=25600`, `dpi_step=50`, polling `[125,250,500,1000]`; `profiles.toml` foi criado com `dpi_levels = [<DPI atual>]` (semeado), e o DPI do mouse **não mudou**. Se o daemon não achar o mouse, registrar `/tmp/ghubd.log` e parar.

- [ ] **Step 2: Conferir a bateria e o monitor de sinais**

```bash
timeout 20 busctl --user monitor org.ghublinux.Daemon 2>&1 | head -20
```

Expected: o monitor abre sem erro. Conferir que `battery_state` em `GetDeviceState` bate com `/sys/class/power_supply/hidpp_battery_0/status` (em minúsculas).

- [ ] **Step 3: Escrita controlada (pedir autorização)**

Com autorização do usuário:

```bash
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 SetDpiLevels sauu 046dc539-1 3 800 1200 1600 1
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 GetDeviceState s 046dc539-1
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 SetActiveDpiLevel su 046dc539-1 0
busctl --user call org.ghublinux.Daemon /org/ghublinux/Daemon org.ghublinux.Daemon1 SetDpiLevels sauu 046dc539-1 1 125 0
```

Expected: a 1ª chamada tem sucesso e `dpi_current` passa a 1200; a 3ª muda para 800; a 4ª (125 DPI, fora da grade) falha com `InvalidArgs` e o DPI continua 800.

- [ ] **Step 4: Documentar**

`docs/hardware/daemon-verification.md`: data, versão do commit, saída resumida de cada chamada acima (sem dados pessoais), o conteúdo de `profiles.toml` após o Step 1 e o resultado de cada "Expected". Registrar qualquer divergência.

`docs/daemon.md`: como compilar e rodar (`cargo run -p ghubd`, `packaging/install-user.sh`, `systemctl --user start|enable ghubd`), arquivo de perfis (`~/.config/ghub-linux/profiles.toml`), tabela dos métodos e sinais D-Bus da Task 2 (nome, argumentos, retorno, erros), exemplos de `busctl`, e as limitações da v1: um perfil global; níveis de DPI são do host; bateria só com um dispositivo por receptor; mouse com fio ainda não tratado; o índice 0xFF do receptor não é sondado.

`README.md`: acrescentar uma seção curta "Daemon" apontando para `docs/daemon.md`.

- [ ] **Step 5: Teste de reconexão (pedir autorização e ação do usuário)**

Com `SetActiveDpiLevel` em 800 e o daemon rodando, o usuário **desliga e liga o mouse pela chave de energia** (ou o deixa dormir). Em até ~10 s o `Devices` mostra `connected=false` e depois `true`, e `GetDeviceState` volta a `dpi_current = 800` (perfil reaplicado). Registrar o resultado em `daemon-verification.md`.

- [ ] **Step 6: Atualizar a spec**

Em `docs/superpowers/specs/2026-10-12-ghub-linux-design.md`:
- Seção 5.2: substituir a lista de métodos por a API final (nomes, argumentos e sinais da Task 2) e citar o nome de barramento `org.ghublinux.Daemon`.
- Seção 5.3: acrescentar que há uma thread gerente única, ciclo de 5 s, a bateria vem do sysfs, o perfil é global e a primeira execução semeia o perfil a partir do dispositivo.

- [ ] **Step 7: Verificação final e commit**

Parar o `ghubd` em primeiro plano. Run: `RUSTFLAGS="-D warnings" cargo test --workspace`
Expected: PASS.

```bash
git add docs README.md
git commit -m "docs: add daemon guide, hardware verification and API spec update"
```

---

## Auto-revisão (feita ao escrever)

- **Cobertura da spec (5.3):** hotplug por udev (Task 8) e ciclo periódico para sono/volta (Task 6); estado desejado reaplicado ao reconectar (Task 6, teste de reconexão); perfis TOML com escrita atômica (Task 3); interface D-Bus (Tasks 2 e 7); bateria baixa como evento (Task 6) — a notificação visual e o ícone ficam no Plano 3; "Iniciar com o sistema" fica no Plano 3 (a unit é entregue na Task 8, mas **não** é habilitada); GUI no Plano 4.
- **Achados do hardware incorporados:** bateria por sysfs; descoberta pelo nó do receptor ignorando nós filhos; SW_ID `0x0E` já no código; endurecimento do transporte (EINTR, POLLHUP, leitura vazia); ignorar feature id 0; validar DPI e polling contra o que o dispositivo anuncia antes de escrever.
- **Adiado de propósito (registrado em `docs/daemon.md`, Task 9):** esvaziar a entrada pendente antes de escrever (a filtragem estrita por SW_ID e função já cobre respostas atrasadas); mouse com fio; perfis por dispositivo; nomes e tipos de relatório curto/longo por descriptor.
- **Consistência de tipos:** `Mouse`/`Capabilities`/`HidppMouse` (Task 5), `Found`/`Backend` (Task 5), `Manager`/`Command`/`Event`/`ManagerError` (Task 6), `DaemonService` (Task 7) e as funções de `ProfileStore` (Task 3) usam os mesmos nomes e assinaturas em todas as tasks. `FakeBackend::with_mouse` devolve o backend e o handle da bateria nas Tasks 6 e 7.
- **Sem placeholders:** todo passo de código traz o código; a única parte que depende do hardware é o conteúdo de `daemon-verification.md` (Task 9, Step 4), cuja estrutura está definida.
- **Risco conhecido do plano:** o código foi escrito contra o `zbus` 5 e o `udev` 0.9 validados num protótipo, mas o `Manager` e os testes são novos; o executor deve corrigir o **código** (não os testes) se algum detalhe de ordem de eventos divergir do descrito nos nomes dos testes, e reportar `DONE_WITH_CONCERNS` com a divergência.
