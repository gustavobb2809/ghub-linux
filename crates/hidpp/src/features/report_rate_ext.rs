//! Taxa de polling estendida (feature `0x8061`), usada por mouses como o G Pro X2.
//!
//! Formatos conferidos num G Pro X2 pelo receptor Lightspeed, medindo a cadência real dos
//! relatórios de movimento (a leitura não serve para isso, veja abaixo):
//! - fn0 `[conexão]`: máscara de taxas permitidas nessa conexão. 0 = receptor (`0x0f`: 125 a
//!   1000 Hz); 1 = cabo USB (`0x7f`: até 8 kHz). Outros valores dão erro de argumento.
//! - fn3 `[índice]`: grava a taxa (índice 0 = 8 ms ... 6 = 125 µs). Tem efeito imediato.
//! - fn2 **não** reflete a taxa efetiva: devolve sempre `03`, mesmo com o mouse a 250 Hz.
//!   Por isso não há leitura confiável; quem chama precisa lembrar o que gravou.

use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::transport::Transport;

pub const EXTENDED_REPORT_RATE: u16 = 0x8061;

/// Tipo de conexão usado nas consultas: o mouse atrás do receptor Lightspeed.
const CONNECTION_RECEIVER: u8 = 0;
const FN_RATES: u8 = 0;
const FN_SET: u8 = 3;

/// Taxa em Hz para cada índice (8 ms, 4 ms, 2 ms, 1 ms, 500 µs, 250 µs, 125 µs).
const RATES_HZ: [u16; 7] = [125, 250, 500, 1000, 2000, 4000, 8000];

/// Taxas que o mouse aceita pelo receptor, em Hz, da menor para a maior.
pub fn supported_hz<T: Transport>(dev: &mut Device<T>) -> Result<Vec<u16>> {
    let p = dev.call(EXTENDED_REPORT_RATE, FN_RATES, &[CONNECTION_RECEIVER])?;
    let mask = u16::from_be_bytes([p[0], p[1]]);
    Ok(RATES_HZ
        .iter()
        .enumerate()
        .filter(|(i, _)| mask & (1 << i) != 0)
        .map(|(_, hz)| *hz)
        .collect())
}

/// Grava a taxa. Só aceita valores da tabela; a checagem contra o que o mouse suporta é de
/// quem chama (`supported_hz`).
pub fn set_hz<T: Transport>(dev: &mut Device<T>, hz: u16) -> Result<()> {
    let Some(index) = RATES_HZ.iter().position(|r| *r == hz) else {
        return Err(HidppError::InvalidArgument("unsupported polling rate"));
    };
    dev.call(EXTENDED_REPORT_RATE, FN_SET, &[index as u8])?;
    Ok(())
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
    fn lists_the_rates_for_the_receiver_connection() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 12, fb(0), &[0x00, 0x0f]));
        let mut d = dev(fake);
        assert_eq!(supported_hz(&mut d), Ok(vec![125, 250, 500, 1000]));
        let t = d.into_transport();
        assert_eq!(t.written[0][3] >> 4, 0, "fn0");
        assert_eq!(t.written[0][4], CONNECTION_RECEIVER);
    }

    #[test]
    fn a_wired_mask_lists_up_to_8khz() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 12, fb(0), &[0x00, 0x7f]));
        assert_eq!(
            supported_hz(&mut dev(fake)),
            Ok(vec![125, 250, 500, 1000, 2000, 4000, 8000])
        );
    }

    #[test]
    fn sets_the_rate_by_index() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 12, fb(3), &[]));
        let mut d = dev(fake);
        assert_eq!(set_hz(&mut d, 500), Ok(()));
        let t = d.into_transport();
        assert_eq!(t.written[0][3] >> 4, 3, "fn3");
        assert_eq!(t.written[0][4], 2, "500 Hz = índice 2");
    }

    #[test]
    fn rejects_rates_outside_the_table() {
        let mut d = dev(FakeTransport::new());
        assert!(set_hz(&mut d, 333).is_err());
        assert!(set_hz(&mut d, 0).is_err());
    }
}
