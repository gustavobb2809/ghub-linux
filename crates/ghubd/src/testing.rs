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
    /// Com o mouse online, leituras falham.
    pub fail_reads: bool,
    /// Com o mouse online, escritas falham (e não são registradas).
    pub fail_writes: bool,
    /// Com o mouse online, `ping` falha.
    pub fail_ping: bool,
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
            fail_reads: false,
            fail_writes: false,
            fail_ping: false,
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
        if s.online && !s.fail_reads { Ok(s.dpi) } else { Err(HidppError::Timeout) }
    }

    fn read_polling_hz(&mut self) -> MouseResult<u16> {
        let s = self.state();
        if s.online && !s.fail_reads { Ok(s.polling) } else { Err(HidppError::Timeout) }
    }

    fn set_dpi(&mut self, dpi: u16) -> MouseResult<u16> {
        let mut s = self.state();
        if !s.online || s.fail_writes {
            return Err(HidppError::Timeout);
        }
        s.dpi = dpi;
        s.dpi_sets.push(dpi);
        Ok(dpi)
    }

    fn set_polling_hz(&mut self, hz: u16) -> MouseResult<()> {
        let mut s = self.state();
        if !s.online || s.fail_writes {
            return Err(HidppError::Timeout);
        }
        s.polling = hz;
        s.polling_sets.push(hz);
        Ok(())
    }

    fn ping(&mut self) -> MouseResult<()> {
        let s = self.state();
        if s.online && !s.fail_ping { Ok(()) } else { Err(HidppError::Timeout) }
    }
}

pub struct FakeBackend {
    pub mice: Vec<(String, String, FakeMouse)>,
    pub battery: Arc<Mutex<Option<BatteryInfo>>>,
    /// Lista `skip` recebida na última varredura.
    pub last_skip: Vec<String>,
}

impl FakeBackend {
    /// Um backend com um único mouse `dev-1` ("G703 LS") e um handle para a bateria.
    pub fn with_mouse(mouse: FakeMouse) -> (Self, Arc<Mutex<Option<BatteryInfo>>>) {
        let battery = Arc::new(Mutex::new(None));
        (
            FakeBackend {
                mice: vec![("dev-1".to_string(), "G703 LS".to_string(), mouse)],
                battery: battery.clone(),
                last_skip: Vec::new(),
            },
            battery,
        )
    }
}

impl Backend for FakeBackend {
    fn scan(&mut self, skip: &[String]) -> Vec<Found> {
        self.last_skip = skip.to_vec();
        self.mice
            .iter()
            .filter(|(id, _, m)| m.state().online && !skip.contains(id))
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
