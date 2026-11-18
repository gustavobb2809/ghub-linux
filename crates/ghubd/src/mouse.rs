use std::path::PathBuf;
use std::time::Duration;

use hidpp::device::{Device, DEFAULT_TIMEOUT};
use hidpp::features::dpi::{self, DpiSupport};
use hidpp::features::onboard::{self, Led, LedKind};
use hidpp::features::dpi_ext::{self, LiftOff};
use hidpp::features::{report_rate, report_rate_ext};
use hidpp::transport::Transport;
use hidpp::HidppError;

const PING_TIMEOUT: Duration = Duration::from_millis(500);
/// O G703 tem um único sensor.
const SENSOR: u8 = 0;

pub type MouseResult<T> = Result<T, HidppError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub dpi: DpiSupport,
    pub polling_hz: Vec<u16>,
    /// Efeitos de LED suportados; vazio sem `0x8100`.
    pub led: Vec<LedKind>,
    /// O lift-off pode ser ajustado (só no DPI estendido `0x2202`).
    pub lift_off: bool,
}

/// Operações que o daemon precisa de um mouse. Implementado sobre HID++ e, nos testes, por um falso.
pub trait Mouse: Send {
    fn capabilities(&mut self) -> MouseResult<Capabilities>;
    fn read_dpi(&mut self) -> MouseResult<u16>;
    fn read_polling_hz(&mut self) -> MouseResult<u16>;
    /// Define o DPI e devolve o valor que o dispositivo aplicou.
    fn set_dpi(&mut self, dpi: u16) -> MouseResult<u16>;
    fn set_polling_hz(&mut self, hz: u16) -> MouseResult<()>;
    /// LED gravado no perfil onboard ativo.
    fn led(&mut self) -> MouseResult<Led>;
    /// Grava o LED no perfil onboard ativo (lento: escreve na flash do mouse).
    fn set_led(&mut self, led: &Led) -> MouseResult<()>;
    /// Lift-off atual; erro se o mouse não permite ajustar.
    fn lift_off(&mut self) -> MouseResult<LiftOff>;
    fn set_lift_off(&mut self, value: LiftOff) -> MouseResult<()>;
    /// Verifica, sem demora, se o dispositivo ainda responde.
    fn ping(&mut self) -> MouseResult<()>;
}

pub struct HidppMouse<T: Transport> {
    device: Device<T>,
    backup_dir: PathBuf,
    /// A cópia do setor original já foi salva nesta sessão do daemon.
    backed_up: bool,
    /// Último LED lido/gravado: evita ~35 leituras HID++ a cada atualização da GUI. Some
    /// só até reconectar (o objeto é recriado) ou até uma gravação falhar (estado incerto).
    led_cache: Option<Led>,
    /// Última taxa gravada no polling estendido. O `0x8061` não informa a taxa efetiva (a
    /// leitura devolve sempre o mesmo valor), então o daemon lembra o que gravou.
    rate_cache: Option<u16>,
}

/// Taxa de um mouse recém-ligado, usada enquanto o daemon ainda não gravou nenhuma.
const POWER_ON_POLLING_HZ: u16 = 1000;

impl<T: Transport> HidppMouse<T> {
    pub fn new(device: Device<T>) -> Self {
        HidppMouse {
            device,
            backup_dir: onboard::default_backup_dir(),
            backed_up: false,
            led_cache: None,
            rate_cache: None,
        }
    }

    pub fn with_backup_dir(mut self, dir: PathBuf) -> Self {
        self.backup_dir = dir;
        self
    }

    pub fn into_device(self) -> Device<T> {
        self.device
    }
}

impl<T: Transport> HidppMouse<T> {
    /// Mouses novos (G Pro X2) trazem o DPI estendido `0x2202` no lugar do `0x2201`.
    fn extended_dpi(&self) -> bool {
        self.device.feature_index(dpi::ADJUSTABLE_DPI).is_err()
            && self.device.feature_index(dpi_ext::EXTENDED_ADJUSTABLE_DPI).is_ok()
    }

    /// Idem para o polling: `0x8061` no lugar do `0x8060`.
    fn extended_polling(&self) -> bool {
        self.device.feature_index(report_rate::REPORT_RATE).is_err()
            && self.device.feature_index(report_rate_ext::EXTENDED_REPORT_RATE).is_ok()
    }
}

impl<T: Transport + Send> Mouse for HidppMouse<T> {
    fn capabilities(&mut self) -> MouseResult<Capabilities> {
        let lift_off = self.extended_dpi()
            && dpi_ext::supports_lift_off(&mut self.device, SENSOR)?;
        let dpi = if self.extended_dpi() {
            // Em modo onboard o firmware recusa a gravação de DPI; o modo host é reaplicado a
            // cada reconexão, pois este método roda toda vez que o mouse volta.
            onboard::set_host_mode(&mut self.device)?;
            dpi_ext::supported(&mut self.device, SENSOR)?
        } else {
            dpi::supported(&mut self.device, SENSOR)?
        };
        let polling_hz = if self.extended_polling() {
            report_rate_ext::supported_hz(&mut self.device)?
        } else {
            report_rate::supported_hz(&mut self.device)?
        };
        Ok(Capabilities {
            dpi,
            polling_hz,
            led: if self.device.feature_index(onboard::ONBOARD_PROFILES).is_ok()
                && !self.extended_dpi()
                && onboard::supports_led(&mut self.device)
            {
                LedKind::ALL.to_vec()
            } else {
                Vec::new()
            },
            lift_off,
        })
    }

    fn read_dpi(&mut self) -> MouseResult<u16> {
        if self.extended_dpi() {
            return Ok(dpi_ext::current(&mut self.device, SENSOR)?.0);
        }
        Ok(dpi::current(&mut self.device, SENSOR)?.0)
    }

    fn read_polling_hz(&mut self) -> MouseResult<u16> {
        if self.extended_polling() {
            return Ok(self.rate_cache.unwrap_or(POWER_ON_POLLING_HZ));
        }
        report_rate::current_hz(&mut self.device)
    }

    fn set_dpi(&mut self, value: u16) -> MouseResult<u16> {
        if self.extended_dpi() {
            return dpi_ext::set(&mut self.device, SENSOR, value);
        }
        dpi::set(&mut self.device, SENSOR, value)
    }

    fn set_polling_hz(&mut self, hz: u16) -> MouseResult<()> {
        if self.extended_polling() {
            report_rate_ext::set_hz(&mut self.device, hz)?;
            self.rate_cache = Some(hz);
            return Ok(());
        }
        report_rate::set_hz(&mut self.device, hz)
    }

    fn led(&mut self) -> MouseResult<Led> {
        if let Some(led) = self.led_cache {
            return Ok(led);
        }
        let led = onboard::read_led(&mut self.device)?;
        self.led_cache = Some(led);
        Ok(led)
    }

    fn set_led(&mut self, led: &Led) -> MouseResult<()> {
        let tag = format!("dev{}", self.device.device_index());
        let HidppMouse { device, backup_dir, backed_up, led_cache, .. } = self;
        let result = onboard::set_led(device, led, |p| {
            if !*backed_up {
                onboard::save_backup(backup_dir, &tag, p)?;
                *backed_up = true;
            }
            Ok(())
        });
        *led_cache = if result.is_ok() { Some(*led) } else { None };
        result
    }

    fn lift_off(&mut self) -> MouseResult<LiftOff> {
        if !self.extended_dpi() {
            return Err(HidppError::FeatureNotSupported(dpi_ext::EXTENDED_ADJUSTABLE_DPI));
        }
        dpi_ext::lift_off(&mut self.device, SENSOR)
    }

    fn set_lift_off(&mut self, value: LiftOff) -> MouseResult<()> {
        if !self.extended_dpi() {
            return Err(HidppError::FeatureNotSupported(dpi_ext::EXTENDED_ADJUSTABLE_DPI));
        }
        dpi_ext::set_lift_off(&mut self.device, SENSOR, value)
    }

    fn ping(&mut self) -> MouseResult<()> {
        self.device.set_timeout(PING_TIMEOUT);
        let result = self.device.ping();
        self.device.set_timeout(DEFAULT_TIMEOUT);
        result.map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hidpp::features::onboard_fake::{sample_profile_sector, MemOnboard};
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
        assert!(caps.led.is_empty(), "sem 0x8100 não há LED");
        assert!(!caps.lift_off, "o G703 (0x2201) não ajusta lift-off");
    }

    /// G Pro X2: DPI/polling estendidos, modo host e nenhum LED, mesmo com `0x8100`.
    fn x2(fake: FakeTransport) -> HidppMouse<FakeTransport> {
        let features = HashMap::from([(0x2202u16, 9u8), (0x8061u16, 12u8), (0x8100u16, 13u8)]);
        HidppMouse::new(Device::with_features(fake, 1, features))
    }

    #[test]
    fn extended_mouse_enters_host_mode_and_lists_its_polling_rates() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 9, fb(1), &[0, 5, 0x0f])); // flags: lift-off ajustável
        fake.push_incoming(long_reply(1, 13, fb(1), &[])); // modo host
        let stream: [u8; 26] = [
            0x00, 0x64, 0xe0, 0x01, 0x00, 0xc8, 0xe0, 0x02, 0x01, 0xf4, 0xe0, 0x05, 0x03, //
            0xe8, 0xe0, 0x0a, 0x07, 0xd0, 0xe0, 0x14, 0x13, 0x88, 0xe0, 0x32, 0x27, 0x10,
        ];
        for (page, chunk) in stream.chunks(13).enumerate() {
            let mut params = vec![0, 0, page as u8];
            params.extend_from_slice(chunk);
            fake.push_incoming(long_reply(1, 9, fb(2), &params));
        }
        fake.push_incoming(long_reply(1, 9, fb(2), &[0, 0, 2])); // página vazia: fim da lista
        fake.push_incoming(long_reply(1, 12, fb(1), &[0x00, 0x7f])); // taxas suportadas
        let mut m = x2(fake);
        let caps = m.capabilities().unwrap();
        assert_eq!(caps.dpi, DpiSupport::Range { min: 100, max: 10000, step: 100 });
        assert_eq!(caps.polling_hz, vec![125, 250, 500, 1000, 2000, 4000, 8000]);
        assert!(caps.led.is_empty(), "o X2 não tem LED");
        assert!(caps.lift_off);
    }

    #[test]
    fn extended_polling_is_remembered_because_the_device_cannot_report_it() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 12, fb(3), &[]));
        let mut m = x2(fake);
        assert_eq!(m.read_polling_hz(), Ok(1000), "sem gravação, vale o padrão de fábrica");
        m.set_polling_hz(500).unwrap();
        assert_eq!(m.read_polling_hz(), Ok(500));
        assert!(m.set_polling_hz(333).is_err());
        assert_eq!(m.read_polling_hz(), Ok(500), "uma gravação recusada não muda o que lembramos");
    }

    #[test]
    fn extended_mouse_reads_dpi_from_the_extended_feature() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 9, fb(5), &[0, 0x03, 0x20, 0x03, 0x20, 0x03, 0x20, 0x03, 0x20, 2]));
        let mut m = x2(fake);
        assert_eq!(m.read_dpi(), Ok(800));
    }

    #[test]
    fn capabilities_list_every_led_kind_when_onboard_profiles_exist() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 11, fb(1), &[0, 0x00, 0x64, 0xE0, 0x32, 0x64, 0x00]));
        fake.push_incoming(long_reply(1, 10, fb(0), &[0b1000_1011]));
        // fn0 de 0x8100: modelo 1, formato 4, 5 perfis, 16 setores de 0x00ff bytes.
        fake.push_incoming(long_reply(1, 8, fb(0), &[1, 4, 1, 5, 1, 6, 0x10, 0x00, 0xff, 0x0a, 4, 0]));
        let features = HashMap::from([(0x2201u16, 11u8), (0x8060u16, 10u8), (0x8100u16, 8u8)]);
        let mut m = HidppMouse::new(Device::with_features(fake, 1, features));
        assert_eq!(m.capabilities().unwrap().led, LedKind::ALL.to_vec());
    }

    #[test]
    fn capabilities_hide_the_led_for_an_unknown_onboard_layout() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 11, fb(1), &[0, 0x00, 0x64, 0xE0, 0x32, 0x64, 0x00]));
        fake.push_incoming(long_reply(1, 10, fb(0), &[0b1000_1011]));
        // Formato de perfil 5: os offsets do G703 não valem.
        fake.push_incoming(long_reply(1, 8, fb(0), &[1, 5, 1, 5, 1, 6, 0x10, 0x00, 0xff, 0x0a, 4, 0]));
        let features = HashMap::from([(0x2201u16, 11u8), (0x8060u16, 10u8), (0x8100u16, 8u8)]);
        let mut m = HidppMouse::new(Device::with_features(fake, 1, features));
        assert!(m.capabilities().unwrap().led.is_empty());
    }

    #[test]
    fn the_led_is_read_from_the_mouse_only_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut once = led_mouse(MemOnboard::new(8, &CYCLE), dir.path());
        once.led().unwrap();
        let reads_once = once.into_device().into_transport().read_calls;
        let mut twice = led_mouse(MemOnboard::new(8, &CYCLE), dir.path());
        twice.led().unwrap();
        twice.led().unwrap();
        assert_eq!(twice.into_device().into_transport().read_calls, reads_once, "a 2ª leitura vem do cache");
    }

    #[test]
    fn set_led_refreshes_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = led_mouse(MemOnboard::new(8, &CYCLE), dir.path());
        m.led().unwrap();
        m.set_led(&Led::Off).unwrap();
        assert_eq!(m.led(), Ok(Led::Off));
    }

    const CYCLE: Led = Led::Cycle { period_ms: 8000, intensity: 0 };

    fn led_mouse(mem: MemOnboard, dir: &std::path::Path) -> HidppMouse<MemOnboard> {
        let features = HashMap::from([(onboard::ONBOARD_PROFILES, 8u8)]);
        HidppMouse::new(Device::with_features(mem, 1, features)).with_backup_dir(dir.to_path_buf())
    }

    #[test]
    fn reads_and_sets_the_led_backing_up_once_per_session() {
        let dir = tempfile::tempdir().unwrap();
        let mut m = led_mouse(MemOnboard::new(8, &CYCLE), dir.path());
        assert_eq!(m.led(), Ok(CYCLE));
        m.set_led(&Led::Fixed { rgb: [255, 0, 0] }).unwrap();
        m.set_led(&Led::Fixed { rgb: [0, 255, 0] }).unwrap();
        assert_eq!(m.led(), Ok(Led::Fixed { rgb: [0, 255, 0] }));
        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(files.len(), 1, "uma cópia por sessão, a do estado original");
        let kept = std::fs::read(files[0].as_ref().unwrap().path()).unwrap();
        assert_eq!(kept, sample_profile_sector(&CYCLE).to_vec());
    }

    #[test]
    fn set_led_failure_when_the_backup_dir_is_unusable_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("arquivo");
        std::fs::write(&blocker, b"x").unwrap();
        // O "diretório" de backup é um arquivo comum: criar a pasta falha.
        let mut m = led_mouse(MemOnboard::new(8, &CYCLE), &blocker.join("sub"));
        assert!(m.set_led(&Led::Off).is_err());
        let mem = m.into_device().into_transport();
        assert!(mem.commits.is_empty());
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
