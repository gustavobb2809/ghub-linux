use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::transport::Transport;

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
