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
    // Um relatório curto (0x10) traz só 3 parâmetros: falta o DPI padrão.
    if p.len() < 5 {
        return Err(HidppError::UnexpectedResponse("short DPI reply"));
    }
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
    use crate::transport::{fb, long_reply, FakeTransport};
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
        // função 1; params: [sensor, min, 0xE000|passo, max]
        fake.push_incoming(long_reply(1, 2, fb(1), &[0, 0x00, 0xC8, 0xE0, 0x32, 0x64, 0x00]));
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
        // função 2; params: [sensor, atual(2), padrão(2)]
        fake.push_incoming(long_reply(1, 2, fb(2), &[0, 0x06, 0x40, 0x03, 0x20]));
        let mut d = dev(fake);
        assert_eq!(current(&mut d, 0), Ok((1600, 800)));
    }

    #[test]
    fn short_current_reply_is_unexpected() {
        let mut fake = FakeTransport::new();
        // Relatório curto: só 3 parâmetros, sem o DPI padrão.
        fake.push_incoming(vec![0x10, 1, 2, fb(2), 0, 0x06, 0x40]);
        let mut d = dev(fake);
        assert_eq!(
            current(&mut d, 0),
            Err(HidppError::UnexpectedResponse("short DPI reply"))
        );
    }

    #[test]
    fn sets_dpi_and_returns_applied_value() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 2, fb(3), &[0, 0x0C, 0x80]));
        let mut d = dev(fake);
        assert_eq!(set(&mut d, 0, 3200), Ok(3200));
        let t = d.into_transport();
        assert_eq!(t.written[0], vec![0x10, 1, 2, fb(3), 0, 0x0C, 0x80]);
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
