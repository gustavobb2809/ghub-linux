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
    if hz == 0 || !1000u16.is_multiple_of(hz) || !(1..=8).contains(&(1000 / hz)) {
        return Err(HidppError::InvalidArgument("unsupported polling rate"));
    }
    dev.call(REPORT_RATE, 2, &[(1000 / hz) as u8])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{fb, long_reply, FakeTransport};
    use std::collections::HashMap;

    fn dev(fake: FakeTransport) -> Device<FakeTransport> {
        Device::with_features(fake, 1, HashMap::from([(REPORT_RATE, 4u8)]))
    }

    #[test]
    fn lists_supported_rates() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 4, fb(0), &[0b1000_1011]));
        let mut d = dev(fake);
        assert_eq!(supported_hz(&mut d), Ok(vec![1000, 500, 250, 125]));
    }

    #[test]
    fn reads_current_rate() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 4, fb(1), &[1]));
        let mut d = dev(fake);
        assert_eq!(current_hz(&mut d), Ok(1000));
    }

    #[test]
    fn zero_interval_is_unexpected() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 4, fb(1), &[0]));
        let mut d = dev(fake);
        assert_eq!(
            current_hz(&mut d),
            Err(HidppError::UnexpectedResponse("report interval is zero"))
        );
    }

    #[test]
    fn sets_rate_as_interval() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 4, fb(2), &[2]));
        let mut d = dev(fake);
        set_hz(&mut d, 500).unwrap();
        let t = d.into_transport();
        assert_eq!(t.written[0], vec![0x10, 1, 4, fb(2), 2, 0, 0]);
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
