//! DPI estendido (feature `0x2202`), usado por mouses mais novos como o G Pro X2.
//!
//! Formatos conferidos num G Pro X2 e na implementação do Solaar:
//! - fn2 `[sensor, eixo, página]`: a lista de DPI vem como um fluxo de bytes em páginas de
//!   13 bytes (a partir do byte 3 da resposta): `min, (0xE000|passo, max)*`, terminado em zero.
//! - fn5 `[sensor]`: `[sensor, X atual, X padrão, Y atual, Y padrão, LOD]`.
//! - fn6 `[sensor, X, Y, LOD]`: grava; só vale com o mouse em modo host (`onboard::set_host_mode`).
//! - fn1 `[sensor]`: `[sensor, nº de níveis, flags]`; o bit 1 das flags indica lift-off ajustável.
//! - Lift-off (LOD): o X2 aceita 1, 2 e 3 (baixa, média, alta) e recusa 0 com erro de argumento.

use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::features::dpi::DpiSupport;
use crate::transport::Transport;

pub const EXTENDED_ADJUSTABLE_DPI: u16 = 0x2202;

const FN_CAPS: u8 = 1;
const FN_LIST: u8 = 2;
const FN_GET: u8 = 5;
const FN_SET: u8 = 6;
const PAGE_LEN: usize = 13;
const MAX_PAGES: u8 = 16;
/// Passo único oferecido à interface (a lista real tem passos de 1 a 200 por faixa).
const UI_STEP: u16 = 100;

/// Distância de levantamento (lift-off): a altura em que o sensor para de rastrear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiftOff {
    Low,
    Medium,
    High,
}

impl LiftOff {
    pub const ALL: [LiftOff; 3] = [LiftOff::Low, LiftOff::Medium, LiftOff::High];

    pub fn name(self) -> &'static str {
        match self {
            LiftOff::Low => "low",
            LiftOff::Medium => "medium",
            LiftOff::High => "high",
        }
    }

    pub fn parse(name: &str) -> Option<LiftOff> {
        LiftOff::ALL.into_iter().find(|l| l.name() == name)
    }

    fn wire(self) -> u8 {
        match self {
            LiftOff::Low => 1,
            LiftOff::Medium => 2,
            LiftOff::High => 3,
        }
    }

    fn from_wire(b: u8) -> Result<LiftOff> {
        LiftOff::ALL
            .into_iter()
            .find(|l| l.wire() == b)
            .ok_or(HidppError::UnexpectedResponse("unknown lift-off value"))
    }
}

/// Faixa de DPI com passo constante: `min, min+passo, ..., max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub min: u16,
    pub max: u16,
    pub step: u16,
}

/// Interpreta o fluxo `min, (0xE000|passo, max)*`; zero ou o fim dos dados encerram a lista.
pub fn parse_segments(stream: &[u8]) -> Result<Vec<Segment>> {
    let words: Vec<u16> = stream
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .take_while(|w| *w != 0)
        .collect();
    let Some(&first) = words.first() else {
        return Err(HidppError::UnexpectedResponse("empty extended DPI list"));
    };
    let mut out = Vec::new();
    let mut min = first;
    let mut rest = words[1..].chunks_exact(2);
    for pair in &mut rest {
        let (marker, max) = (pair[0], pair[1]);
        if marker & 0xE000 != 0xE000 || marker & 0x1FFF == 0 || max < min {
            return Err(HidppError::UnexpectedResponse("malformed extended DPI list"));
        }
        out.push(Segment { min, max, step: marker & 0x1FFF });
        min = max;
    }
    if out.is_empty() {
        return Err(HidppError::UnexpectedResponse("malformed extended DPI list"));
    }
    Ok(out)
}

fn allowed(segments: &[Segment], dpi: u16) -> bool {
    segments
        .iter()
        .any(|s| dpi >= s.min && dpi <= s.max && (dpi - s.min).is_multiple_of(s.step))
}

/// Maior faixa uniforme de passo `UI_STEP` que começa no mínimo e só contém valores válidos.
/// A interface e o daemon trabalham com `min/max/passo`; passos de 100 cobrem 100 a 20000 DPI
/// no X2, sem oferecer valores que o mouse recusaria.
pub fn to_support(segments: &[Segment]) -> Result<DpiSupport> {
    let top = segments.iter().map(|s| s.max).max().unwrap_or(0);
    let min = segments
        .iter()
        .map(|s| s.min)
        .min()
        .unwrap_or(0)
        .next_multiple_of(UI_STEP);
    if min == 0 || !allowed(segments, min) {
        return Err(HidppError::UnexpectedResponse("no usable extended DPI range"));
    }
    let mut max = min;
    while let Some(next) = max.checked_add(UI_STEP) {
        if next > top || !allowed(segments, next) {
            break;
        }
        max = next;
    }
    Ok(DpiSupport::Range { min, max, step: UI_STEP })
}

pub fn supported<T: Transport>(dev: &mut Device<T>, sensor: u8) -> Result<DpiSupport> {
    let mut stream = Vec::new();
    for page in 0..MAX_PAGES {
        let p = dev.call(EXTENDED_ADJUSTABLE_DPI, FN_LIST, &[sensor, 0, page])?;
        if p.len() < 3 + PAGE_LEN {
            return Err(HidppError::UnexpectedResponse("short extended DPI page"));
        }
        let chunk = &p[3..3 + PAGE_LEN];
        stream.extend_from_slice(chunk);
        // Uma palavra zero no fim da página encerra a lista; evita pedir páginas vazias.
        if let Ok(segments) = parse_segments(&stream) {
            if chunk.ends_with(&[0, 0]) || chunk.iter().all(|b| *b == 0) {
                return to_support(&segments);
            }
        }
    }
    to_support(&parse_segments(&stream)?)
}

fn read_params<T: Transport>(dev: &mut Device<T>, sensor: u8) -> Result<Vec<u8>> {
    let p = dev.call(EXTENDED_ADJUSTABLE_DPI, FN_GET, &[sensor])?;
    if p.len() < 10 {
        return Err(HidppError::UnexpectedResponse("short extended DPI reply"));
    }
    Ok(p)
}

/// O sensor permite ajustar o lift-off (bit 1 das flags de `fn1`).
pub fn supports_lift_off<T: Transport>(dev: &mut Device<T>, sensor: u8) -> Result<bool> {
    let p = dev.call(EXTENDED_ADJUSTABLE_DPI, FN_CAPS, &[sensor])?;
    Ok(p[2] & 0x02 != 0)
}

pub fn lift_off<T: Transport>(dev: &mut Device<T>, sensor: u8) -> Result<LiftOff> {
    LiftOff::from_wire(read_params(dev, sensor)?[9])
}

/// Grava o lift-off mantendo o DPI atual de X e Y.
pub fn set_lift_off<T: Transport>(dev: &mut Device<T>, sensor: u8, value: LiftOff) -> Result<()> {
    let p = read_params(dev, sensor)?;
    dev.call(
        EXTENDED_ADJUSTABLE_DPI,
        FN_SET,
        &[sensor, p[1], p[2], p[5], p[6], value.wire()],
    )?;
    Ok(())
}

/// Devolve `(dpi_atual, dpi_padrão)` do eixo X.
pub fn current<T: Transport>(dev: &mut Device<T>, sensor: u8) -> Result<(u16, u16)> {
    let p = read_params(dev, sensor)?;
    Ok((u16::from_be_bytes([p[1], p[2]]), u16::from_be_bytes([p[3], p[4]])))
}

/// Define o DPI (X e Y iguais, LOD preservado) e devolve o valor que o mouse aplicou.
pub fn set<T: Transport>(dev: &mut Device<T>, sensor: u8, dpi: u16) -> Result<u16> {
    let lod = read_params(dev, sensor)?[9];
    let [hi, lo] = dpi.to_be_bytes();
    let p = dev.call(EXTENDED_ADJUSTABLE_DPI, FN_SET, &[sensor, hi, lo, hi, lo, lod])?;
    Ok(u16::from_be_bytes([p[1], p[2]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{fb, long_reply, FakeTransport};
    use std::collections::HashMap;

    /// Fluxo real lido do G Pro X2 (páginas 0 a 2).
    const STREAM: [u8; 39] = [
        0x00, 0x64, 0xe0, 0x01, 0x00, 0xc8, 0xe0, 0x02, 0x01, 0xf4, 0xe0, 0x05, 0x03, // página 0
        0xe8, 0xe0, 0x0a, 0x07, 0xd0, 0xe0, 0x14, 0x13, 0x88, 0xe0, 0x32, 0x27, 0x10, // página 1
        0xe0, 0x64, 0x4e, 0x20, 0xe0, 0x7d, 0x7d, 0x00, 0xe0, 0xc8, 0xab, 0xe0, 0x00, // página 2
    ];

    fn dev(fake: FakeTransport) -> Device<FakeTransport> {
        Device::with_features(fake, 1, HashMap::from([(EXTENDED_ADJUSTABLE_DPI, 9u8)]))
    }

    fn page_reply(page: u8, chunk: &[u8]) -> Vec<u8> {
        let mut params = vec![0, 0, page];
        params.extend_from_slice(chunk);
        long_reply(1, 9, fb(FN_LIST), &params)
    }

    #[test]
    fn parses_the_real_x2_stream() {
        let s = parse_segments(&STREAM).unwrap();
        assert_eq!(s.len(), 9);
        assert_eq!(s[0], Segment { min: 100, max: 200, step: 1 });
        assert_eq!(s[2], Segment { min: 500, max: 1000, step: 5 });
        assert_eq!(s[8], Segment { min: 32000, max: 44000, step: 200 });
    }

    #[test]
    fn rejects_empty_and_malformed_streams() {
        assert!(parse_segments(&[0, 0]).is_err());
        assert!(parse_segments(&[0x00, 0x64]).is_err());
        assert!(parse_segments(&[0x00, 0x64, 0x00, 0x01, 0x00, 0xc8]).is_err());
    }

    #[test]
    fn uniform_support_only_offers_values_the_mouse_accepts() {
        let s = parse_segments(&STREAM).unwrap();
        assert_eq!(to_support(&s), Ok(DpiSupport::Range { min: 100, max: 20000, step: 100 }));
        assert!(allowed(&s, 20000));
        assert!(!allowed(&s, 20100), "20000 a 32000 tem passo 125");
    }

    #[test]
    fn reads_all_pages_until_the_terminator() {
        let mut fake = FakeTransport::new();
        for (page, chunk) in STREAM.chunks(PAGE_LEN).enumerate() {
            fake.push_incoming(page_reply(page as u8, chunk));
        }
        fake.push_incoming(page_reply(3, &[0; PAGE_LEN]));
        let mut d = dev(fake);
        assert_eq!(supported(&mut d, 0), Ok(DpiSupport::Range { min: 100, max: 20000, step: 100 }));
    }

    #[test]
    fn reads_current_and_default() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 9, fb(FN_GET), &[0, 0x04, 0xb0, 0x03, 0x20, 0x04, 0xb0, 0x03, 0x20, 2]));
        let mut d = dev(fake);
        assert_eq!(current(&mut d, 0), Ok((1200, 800)));
    }

    #[test]
    fn lift_off_names_and_wire_values() {
        assert_eq!(LiftOff::parse("medium"), Some(LiftOff::Medium));
        assert_eq!(LiftOff::parse("x"), None);
        assert_eq!(LiftOff::from_wire(1), Ok(LiftOff::Low));
        assert_eq!(LiftOff::from_wire(3), Ok(LiftOff::High));
        assert!(LiftOff::from_wire(0).is_err(), "o mouse recusa 0");
    }

    #[test]
    fn detects_lift_off_support_from_the_flags() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 9, fb(FN_CAPS), &[0, 5, 0x0f]));
        fake.push_incoming(long_reply(1, 9, fb(FN_CAPS), &[0, 5, 0x01]));
        let mut d = dev(fake);
        assert_eq!(supports_lift_off(&mut d, 0), Ok(true));
        assert_eq!(supports_lift_off(&mut d, 0), Ok(false));
    }

    #[test]
    fn set_lift_off_keeps_the_current_dpi_of_both_axes() {
        let mut fake = FakeTransport::new();
        // X 1400 (padrão 800), Y 1400 (padrão 800), LOD 2.
        fake.push_incoming(long_reply(1, 9, fb(FN_GET), &[0, 0x05, 0x78, 0x03, 0x20, 0x05, 0x78, 0x03, 0x20, 2]));
        fake.push_incoming(long_reply(1, 9, fb(FN_SET), &[0, 0x05, 0x78, 0x05, 0x78, 3]));
        fake.push_incoming(long_reply(1, 9, fb(FN_GET), &[0, 0x05, 0x78, 0x03, 0x20, 0x05, 0x78, 0x03, 0x20, 3]));
        let mut d = dev(fake);
        assert_eq!(set_lift_off(&mut d, 0, LiftOff::High), Ok(()));
        assert_eq!(lift_off(&mut d, 0), Ok(LiftOff::High));
        let t = d.into_transport();
        assert_eq!(&t.written[1][4..10], &[0, 0x05, 0x78, 0x05, 0x78, 3]);
    }

    #[test]
    fn set_keeps_lod_and_returns_the_applied_value() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 9, fb(FN_GET), &[0, 0x03, 0x20, 0x03, 0x20, 0x03, 0x20, 0x03, 0x20, 2]));
        fake.push_incoming(long_reply(1, 9, fb(FN_SET), &[0, 0x04, 0xb0, 0x04, 0xb0, 2]));
        let mut d = dev(fake);
        assert_eq!(set(&mut d, 0, 1200), Ok(1200));
        let t = d.into_transport();
        let sent = t.written.last().unwrap();
        assert_eq!(&sent[4..10], &[0, 0x04, 0xb0, 0x04, 0xb0, 2]);
    }
}
