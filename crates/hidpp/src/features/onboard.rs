//! Perfis onboard (`0x8100`): leitura/gravação do setor do perfil ativo e o LED que ele guarda.
//! Formato conferido no G703 LS (docs/onboard-profiles.md).

use crate::error::{HidppError, Result};

pub const ONBOARD_PROFILES: u16 = 0x8100;
pub const SECTOR_SIZE: usize = 255;
pub type Sector = [u8; SECTOR_SIZE];

const CRC_AT: usize = SECTOR_SIZE - 2;
/// Quatro entradas de LED de 11 bytes no fim do perfil; hoje o mouse as mantém iguais.
pub const LED_OFFSETS: [usize; 4] = [0xd0, 0xdb, 0xe6, 0xf1];
pub const LED_LEN: usize = 11;
/// Forma de onda da respiração (layout do libratbag; validar no hardware).
pub const BREATHING_WAVEFORM: u8 = 0x00;

const MODE_OFF: u8 = 0x00;
const MODE_FIXED: u8 = 0x01;
const MODE_CYCLE: u8 = 0x03;
const MODE_BREATHING: u8 = 0x0a;

/// CRC-16/CCITT-FALSE (poli 0x1021, init 0xFFFF).
pub fn crc16(data: &[u8]) -> u16 {
    let mut c: u16 = 0xffff;
    for &x in data {
        c ^= u16::from(x) << 8;
        for _ in 0..8 {
            c = if c & 0x8000 != 0 { (c << 1) ^ 0x1021 } else { c << 1 };
        }
    }
    c
}

pub fn verify(s: &Sector) -> bool {
    crc16(&s[..CRC_AT]).to_be_bytes() == [s[CRC_AT], s[CRC_AT + 1]]
}

pub fn seal(s: &mut Sector) {
    let c = crc16(&s[..CRC_AT]).to_be_bytes();
    s[CRC_AT] = c[0];
    s[CRC_AT + 1] = c[1];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LedKind {
    Off,
    Fixed,
    Cycle,
    Breathing,
}

impl LedKind {
    pub const ALL: [LedKind; 4] = [LedKind::Off, LedKind::Fixed, LedKind::Cycle, LedKind::Breathing];

    pub fn name(self) -> &'static str {
        match self {
            LedKind::Off => "off",
            LedKind::Fixed => "fixed",
            LedKind::Cycle => "cycle",
            LedKind::Breathing => "breathing",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Led {
    Off,
    Fixed { rgb: [u8; 3] },
    /// `intensity` 0 = 100 %.
    Cycle { period_ms: u16, intensity: u8 },
    Breathing { rgb: [u8; 3], period_ms: u16, waveform: u8, intensity: u8 },
}

fn color_text(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

fn parse_color(text: &str) -> std::result::Result<[u8; 3], String> {
    let bad = || format!("cor inválida: use o formato #RRGGBB (recebido \"{text}\")");
    let hex = text.strip_prefix('#').ok_or_else(bad)?;
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let v = u32::from_str_radix(hex, 16).map_err(|_| bad())?;
    Ok([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

impl Led {
    pub fn kind(&self) -> LedKind {
        match self {
            Led::Off => LedKind::Off,
            Led::Fixed { .. } => LedKind::Fixed,
            Led::Cycle { .. } => LedKind::Cycle,
            Led::Breathing { .. } => LedKind::Breathing,
        }
    }

    pub fn parse(raw: &[u8; LED_LEN]) -> Result<Led> {
        Ok(match raw[0] {
            MODE_OFF => Led::Off,
            MODE_FIXED => Led::Fixed { rgb: [raw[1], raw[2], raw[3]] },
            MODE_CYCLE => Led::Cycle { period_ms: u16::from_be_bytes([raw[6], raw[7]]), intensity: raw[8] },
            MODE_BREATHING => Led::Breathing {
                rgb: [raw[1], raw[2], raw[3]],
                period_ms: u16::from_be_bytes([raw[4], raw[5]]),
                waveform: raw[6],
                intensity: raw[7],
            },
            _ => return Err(HidppError::UnexpectedResponse("unknown LED mode")),
        })
    }

    pub fn encode(&self) -> [u8; LED_LEN] {
        let mut b = [0u8; LED_LEN];
        match *self {
            Led::Off => {}
            Led::Fixed { rgb } => {
                b[0] = MODE_FIXED;
                b[1..4].copy_from_slice(&rgb);
            }
            Led::Cycle { period_ms, intensity } => {
                b[0] = MODE_CYCLE;
                b[6..8].copy_from_slice(&period_ms.to_be_bytes());
                b[8] = intensity;
            }
            Led::Breathing { rgb, period_ms, waveform, intensity } => {
                b[0] = MODE_BREATHING;
                b[1..4].copy_from_slice(&rgb);
                b[4..6].copy_from_slice(&period_ms.to_be_bytes());
                b[6] = waveform;
                b[7] = intensity;
            }
        }
        b
    }

    /// Monta um `Led` a partir do que chega por D-Bus/CLI. `color` só vale em fixed/breathing e
    /// `period_ms` só em cycle/breathing; o resto é ignorado.
    pub fn from_parts(
        effect: &str,
        color: &str,
        period_ms: u32,
        intensity: u32,
    ) -> std::result::Result<Led, String> {
        let period = || match u16::try_from(period_ms) {
            Ok(p) if p > 0 => Ok(p),
            _ => Err(format!("período inválido: {period_ms} ms (use de 1 a 65535)")),
        };
        let level = || match u8::try_from(intensity) {
            Ok(i) if i <= 100 => Ok(i),
            _ => Err(format!("intensidade inválida: {intensity} (use de 0 a 100)")),
        };
        match effect {
            "off" => Ok(Led::Off),
            "fixed" => Ok(Led::Fixed { rgb: parse_color(color)? }),
            "cycle" => Ok(Led::Cycle { period_ms: period()?, intensity: level()? }),
            "breathing" => Ok(Led::Breathing {
                rgb: parse_color(color)?,
                period_ms: period()?,
                waveform: BREATHING_WAVEFORM,
                intensity: level()?,
            }),
            other => Err(format!("efeito desconhecido: \"{other}\"")),
        }
    }

    /// `(efeito, cor "#RRGGBB" ou "", período, intensidade)`; inverso de `from_parts`.
    pub fn parts(&self) -> (&'static str, String, u16, u8) {
        match *self {
            Led::Off => ("off", String::new(), 0, 0),
            Led::Fixed { rgb } => ("fixed", color_text(rgb), 0, 0),
            Led::Cycle { period_ms, intensity } => ("cycle", String::new(), period_ms, intensity),
            Led::Breathing { rgb, period_ms, intensity, .. } => ("breathing", color_text(rgb), period_ms, intensity),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_matches_the_ccitt_false_check_value() {
        assert_eq!(crc16(b"123456789"), 0x29B1);
    }

    #[test]
    fn seal_then_verify_roundtrips_and_detects_corruption() {
        let mut s: Sector = [0xff; SECTOR_SIZE];
        s[3] = 0x42;
        assert!(!verify(&s));
        seal(&mut s);
        assert!(verify(&s));
        s[10] ^= 1;
        assert!(!verify(&s));
    }

    #[test]
    fn parses_the_cycle_entry_read_from_the_g703() {
        // Bytes reais do setor 1 do G703 LS (docs/onboard-profiles.md).
        let raw = [0x03, 0, 0, 0, 0, 0, 0x1f, 0x40, 0, 0, 0];
        assert_eq!(Led::parse(&raw), Ok(Led::Cycle { period_ms: 8000, intensity: 0 }));
        assert_eq!(Led::Cycle { period_ms: 8000, intensity: 0 }.encode(), raw);
    }

    #[test]
    fn fixed_and_off_layouts() {
        let fixed = Led::Fixed { rgb: [0xff, 0x00, 0x00] };
        assert_eq!(fixed.encode(), [0x01, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(Led::parse(&fixed.encode()), Ok(fixed));
        assert_eq!(Led::Off.encode(), [0u8; 11]);
        assert_eq!(Led::parse(&[0u8; 11]), Ok(Led::Off));
    }

    #[test]
    fn breathing_roundtrips() {
        let b = Led::Breathing { rgb: [1, 2, 3], period_ms: 4000, waveform: 1, intensity: 50 };
        assert_eq!(b.encode(), [0x0a, 1, 2, 3, 0x0f, 0xa0, 1, 50, 0, 0, 0]);
        assert_eq!(Led::parse(&b.encode()), Ok(b));
    }

    #[test]
    fn unknown_mode_is_an_error() {
        assert_eq!(
            Led::parse(&[0x7f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            Err(HidppError::UnexpectedResponse("unknown LED mode"))
        );
    }

    #[test]
    fn from_parts_accepts_valid_input_and_normalizes_color() {
        assert_eq!(
            Led::from_parts("fixed", "#fe8019", 0, 0),
            Ok(Led::Fixed { rgb: [0xfe, 0x80, 0x19] })
        );
        assert_eq!(Led::from_parts("off", "", 0, 0), Ok(Led::Off));
        assert_eq!(
            Led::from_parts("cycle", "", 8000, 0),
            Ok(Led::Cycle { period_ms: 8000, intensity: 0 })
        );
        assert_eq!(
            Led::from_parts("breathing", "#00ff00", 4000, 100),
            Ok(Led::Breathing { rgb: [0, 255, 0], period_ms: 4000, waveform: BREATHING_WAVEFORM, intensity: 100 })
        );
    }

    #[test]
    fn from_parts_rejects_bad_input() {
        for (e, c, p, i) in [
            ("sparkle", "", 0, 0),
            ("fixed", "", 0, 0),
            ("fixed", "#GGGGGG", 0, 0),
            ("fixed", "#fff", 0, 0),
            ("fixed", "fe8019", 0, 0),
            ("cycle", "", 0, 0),
            ("cycle", "", 70000, 0),
            ("breathing", "#ff0000", 4000, 101),
        ] {
            assert!(Led::from_parts(e, c, p, i).is_err(), "{e} {c} {p} {i}");
        }
    }

    #[test]
    fn parts_is_the_inverse_of_from_parts() {
        for led in [
            Led::Off,
            Led::Fixed { rgb: [0xfe, 0x80, 0x19] },
            Led::Cycle { period_ms: 8000, intensity: 0 },
            Led::Breathing { rgb: [0, 255, 0], period_ms: 4000, waveform: BREATHING_WAVEFORM, intensity: 100 },
        ] {
            let (e, c, p, i) = led.parts();
            assert_eq!(Led::from_parts(e, &c, u32::from(p), u32::from(i)), Ok(led));
        }
        assert_eq!(Led::Fixed { rgb: [0xfe, 0x80, 0x19] }.parts().1, "#FE8019");
    }
}
