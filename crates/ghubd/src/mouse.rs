use std::path::PathBuf;
use std::time::Duration;

use hidpp::device::{Device, DEFAULT_TIMEOUT};
use hidpp::features::dpi::{self, DpiSupport};
use hidpp::features::onboard::{self, Led, LedKind};
use hidpp::features::report_rate;
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
    /// Verifica, sem demora, se o dispositivo ainda responde.
    fn ping(&mut self) -> MouseResult<()>;
}

pub struct HidppMouse<T: Transport> {
    device: Device<T>,
    backup_dir: PathBuf,
    /// A cópia do setor original já foi salva nesta sessão do daemon.
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

    pub fn into_device(self) -> Device<T> {
        self.device
    }
}

impl<T: Transport + Send> Mouse for HidppMouse<T> {
    fn capabilities(&mut self) -> MouseResult<Capabilities> {
        Ok(Capabilities {
            dpi: dpi::supported(&mut self.device, SENSOR)?,
            polling_hz: report_rate::supported_hz(&mut self.device)?,
            led: if self.device.feature_index(onboard::ONBOARD_PROFILES).is_ok() {
                LedKind::ALL.to_vec()
            } else {
                Vec::new()
            },
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
    }

    #[test]
    fn capabilities_list_every_led_kind_when_onboard_profiles_exist() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 11, fb(1), &[0, 0x00, 0x64, 0xE0, 0x32, 0x64, 0x00]));
        fake.push_incoming(long_reply(1, 10, fb(0), &[0b1000_1011]));
        let features = HashMap::from([(0x2201u16, 11u8), (0x8060u16, 10u8), (0x8100u16, 8u8)]);
        let mut m = HidppMouse::new(Device::with_features(fake, 1, features));
        assert_eq!(m.capabilities().unwrap().led, LedKind::ALL.to_vec());
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
