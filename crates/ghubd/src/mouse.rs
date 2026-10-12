use std::time::Duration;

use hidpp::device::{Device, DEFAULT_TIMEOUT};
use hidpp::features::dpi::{self, DpiSupport};
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
