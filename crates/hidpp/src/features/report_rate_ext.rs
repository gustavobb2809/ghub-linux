//! Taxa de polling estendida (feature `0x8061`), usada por mouses como o G Pro X2.
//!
//! Só leitura: no X2 conectado pelo receptor a gravação (fn3) é aceita mas não altera a taxa,
//! e o formato certo dos parâmetros de escrita ainda não é conhecido.
//! fn1 devolve a máscara de taxas suportadas; fn2, o índice da taxa atual.

use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::transport::Transport;

pub const EXTENDED_REPORT_RATE: u16 = 0x8061;

/// Taxa em Hz para cada índice (8 ms, 4 ms, 2 ms, 1 ms, 500 µs, 250 µs, 125 µs).
const RATES_HZ: [u16; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];

pub fn supported_hz<T: Transport>(dev: &mut Device<T>) -> Result<Vec<u16>> {
    let p = dev.call(EXTENDED_REPORT_RATE, 1, &[])?;
    let mask = u16::from_be_bytes([p[0], p[1]]);
    Ok(RATES_HZ
        .iter()
        .enumerate()
        .filter(|(i, _)| mask & (1 << i) != 0)
        .map(|(_, hz)| *hz)
        .collect())
}

pub fn current_hz<T: Transport>(dev: &mut Device<T>) -> Result<u16> {
    let p = dev.call(EXTENDED_REPORT_RATE, 2, &[])?;
    RATES_HZ
        .get(usize::from(p[0]))
        .copied()
        .ok_or(HidppError::UnexpectedResponse("unknown report rate index"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{fb, long_reply, FakeTransport};
    use std::collections::HashMap;

    fn dev(fake: FakeTransport) -> Device<FakeTransport> {
        Device::with_features(fake, 1, HashMap::from([(EXTENDED_REPORT_RATE, 12u8)]))
    }

    #[test]
    fn lists_the_rates_in_the_mask() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 12, fb(1), &[0x00, 0x7f]));
        assert_eq!(
            supported_hz(&mut dev(fake)),
            Ok(vec![125, 250, 500, 1000, 2000, 4000, 8000])
        );
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 12, fb(1), &[0x00, 0x0f]));
        assert_eq!(supported_hz(&mut dev(fake)), Ok(vec![125, 250, 500, 1000]));
    }

    #[test]
    fn reads_the_current_rate_from_its_index() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 12, fb(2), &[3]));
        assert_eq!(current_hz(&mut dev(fake)), Ok(1000));
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 12, fb(2), &[9]));
        assert!(current_hz(&mut dev(fake)).is_err());
    }
}
