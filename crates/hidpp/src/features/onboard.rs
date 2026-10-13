//! Perfis onboard (`0x8100`): leitura/gravação do setor do perfil ativo e o LED que ele guarda.
//! Formato conferido no G703 LS (docs/onboard-profiles.md).

use std::path::{Path, PathBuf};

use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::transport::Transport;

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
    /// Modo gravado por outro programa que não sabemos editar; nunca é oferecido.
    Other,
}

impl LedKind {
    pub const ALL: [LedKind; 4] = [LedKind::Off, LedKind::Fixed, LedKind::Cycle, LedKind::Breathing];

    pub fn name(self) -> &'static str {
        match self {
            LedKind::Off => "off",
            LedKind::Fixed => "fixed",
            LedKind::Cycle => "cycle",
            LedKind::Breathing => "breathing",
            LedKind::Other => "other",
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
    /// Modo desconhecido lido do mouse (ex.: efeitos que o G HUB do Windows grava). Só leitura:
    /// o daemon recusa gravá-lo.
    Other { mode: u8 },
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
            Led::Other { .. } => LedKind::Other,
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
            mode => Led::Other { mode },
        })
    }

    pub fn encode(&self) -> [u8; LED_LEN] {
        let mut b = [0u8; LED_LEN];
        match *self {
            Led::Off => {}
            Led::Other { mode } => b[0] = mode,
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
            Led::Other { .. } => ("other", String::new(), 0, 0),
        }
    }
}

const FN_DESCRIBE: u8 = 0;
const FN_RELOAD: u8 = 3;
const FN_CURRENT: u8 = 4;
const FN_READ: u8 = 5;
const FN_WRITE_START: u8 = 6;
const FN_WRITE: u8 = 7;
const FN_WRITE_END: u8 = 8;

pub fn read_sector<T: Transport>(dev: &mut Device<T>, sector: u16) -> Result<Sector> {
    let mut out: Sector = [0; SECTOR_SIZE];
    let [sh, sl] = sector.to_be_bytes();
    // 0,16,…,224 cobrem 0..240; a última leitura em 239 cobre o resto (sobrepõe 1 byte).
    for off in (0u16..240).step_by(16).chain([SECTOR_SIZE as u16 - 16]) {
        let [oh, ol] = off.to_be_bytes();
        let r = dev.call(ONBOARD_PROFILES, FN_READ, &[sh, sl, oh, ol])?;
        let at = usize::from(off);
        let n = 16.min(SECTOR_SIZE - at);
        out[at..at + n].copy_from_slice(&r[..n]);
    }
    Ok(out)
}

pub fn write_sector<T: Transport>(dev: &mut Device<T>, sector: u16, bytes: &Sector) -> Result<()> {
    let [sh, sl] = sector.to_be_bytes();
    dev.call(ONBOARD_PROFILES, FN_WRITE_START, &[sh, sl, 0, 0, 0, SECTOR_SIZE as u8])?;
    for chunk in bytes.chunks(16) {
        let mut block = [0xffu8; 16];
        block[..chunk.len()].copy_from_slice(chunk);
        dev.call(ONBOARD_PROFILES, FN_WRITE, &block)?;
    }
    dev.call(ONBOARD_PROFILES, FN_WRITE_END, &[])?;
    Ok(())
}

/// Perfil onboard em uso: número (1 a N), setor onde mora e seu conteúdo (CRC já conferido).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveProfile {
    pub number: u8,
    pub sector: u16,
    pub bytes: Sector,
}

impl ActiveProfile {
    fn entry(&self, i: usize) -> [u8; LED_LEN] {
        let o = LED_OFFSETS[i];
        self.bytes[o..o + LED_LEN].try_into().expect("11 bytes")
    }

    /// O LED da primeira entrada (as quatro são mantidas iguais).
    pub fn led(&self) -> Result<Led> {
        Led::parse(&self.entry(0))
    }

    /// Cópia com `led` nas quatro entradas e o CRC refeito; o resto dos bytes não muda.
    pub fn with_led(&self, led: &Led) -> ActiveProfile {
        let mut n = self.clone();
        for o in LED_OFFSETS {
            n.bytes[o..o + LED_LEN].copy_from_slice(&led.encode());
        }
        seal(&mut n.bytes);
        n
    }
}

/// Lê o perfil ativo: `fn4` dá o número (1-based; confirmado só para o perfil 1) e o diretório
/// (setor 0, entradas de 4 bytes `[setor(2), habilitado, 0]`) diz onde ele mora.
pub fn read_active_profile<T: Transport>(dev: &mut Device<T>) -> Result<ActiveProfile> {
    let number = dev.call(ONBOARD_PROFILES, FN_CURRENT, &[])?[1];
    if number == 0 {
        return Err(HidppError::UnexpectedResponse("no active onboard profile"));
    }
    let dir = read_sector(dev, 0)?;
    if !verify(&dir) {
        return Err(HidppError::BadChecksum);
    }
    let at = (usize::from(number) - 1) * 4;
    if at + 2 > CRC_AT {
        return Err(HidppError::UnexpectedResponse("onboard profile outside the directory"));
    }
    let sector = u16::from_be_bytes([dir[at], dir[at + 1]]);
    if sector == 0xffff {
        return Err(HidppError::UnexpectedResponse("onboard profile not in the directory"));
    }
    let bytes = read_sector(dev, sector)?;
    if !verify(&bytes) {
        return Err(HidppError::BadChecksum);
    }
    Ok(ActiveProfile { number, sector, bytes })
}

pub fn read_led<T: Transport>(dev: &mut Device<T>) -> Result<Led> {
    read_active_profile(dev)?.led()
}

/// Grava `led` no perfil ativo. Ordem: ler (CRC válido) → `backup` (erro aborta) → gravar →
/// reler e comparar (se diferir, regrava o original e devolve `VerifyFailed`) → recarregar.
/// Se o LED já é o pedido, não faz nada.
pub fn set_led<T: Transport>(
    dev: &mut Device<T>,
    led: &Led,
    backup: impl FnOnce(&ActiveProfile) -> std::io::Result<()>,
) -> Result<()> {
    let original = read_active_profile(dev)?;
    let new = original.with_led(led);
    if new.bytes == original.bytes {
        return Ok(());
    }
    backup(&original)?;
    let outcome = write_sector(dev, new.sector, &new.bytes)
        .and_then(|()| read_sector(dev, new.sector))
        .and_then(|back| if back == new.bytes { Ok(()) } else { Err(HidppError::VerifyFailed) });
    if let Err(first) = outcome {
        // Qualquer falha depois de começar a gravar deixa o setor em estado incerto:
        // tenta devolver o original e confirma lendo de volta.
        let restored = write_sector(dev, original.sector, &original.bytes)
            .and_then(|()| read_sector(dev, original.sector))
            .is_ok_and(|back| back == original.bytes);
        return Err(if restored { first } else { HidppError::RestoreFailed });
    }
    dev.call(ONBOARD_PROFILES, FN_RELOAD, &[0, new.number])?;
    Ok(())
}

/// Regrava `bytes` (de uma cópia de segurança) no `sector`, confere lendo de volta e recarrega o
/// perfil ativo. Não lê o setor atual: serve justamente para quando ele está corrompido.
pub fn restore_sector<T: Transport>(dev: &mut Device<T>, sector: u16, bytes: &Sector) -> Result<()> {
    if !verify(bytes) {
        return Err(HidppError::BadChecksum);
    }
    write_sector(dev, sector, bytes)?;
    if read_sector(dev, sector)? != *bytes {
        return Err(HidppError::VerifyFailed);
    }
    // Recarregar é só cortesia: se falhar, a restauração em si já foi confirmada.
    if let Ok(p) = dev.call(ONBOARD_PROFILES, FN_CURRENT, &[]) {
        if p[1] != 0 {
            let _ = dev.call(ONBOARD_PROFILES, FN_RELOAD, &[0, p[1]]);
        }
    }
    Ok(())
}

/// Descrição da memória onboard (`fn0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Description {
    pub memory_model: u8,
    pub profile_format: u8,
    pub sector_count: u8,
    pub sector_size: u16,
}

pub fn describe<T: Transport>(dev: &mut Device<T>) -> Result<Description> {
    let r = dev.call(ONBOARD_PROFILES, FN_DESCRIBE, &[])?;
    Ok(Description {
        memory_model: r[0],
        profile_format: r[1],
        sector_count: r[6],
        sector_size: u16::from_be_bytes([r[7], r[8]]),
    })
}

/// Só o layout conferido no G703 LS (modelo de memória 1, formato de perfil 4, setores de 255
/// bytes) tem os offsets de LED conhecidos; qualquer outro dispositivo fica sem LED.
pub fn supports_led<T: Transport>(dev: &mut Device<T>) -> bool {
    matches!(
        describe(dev),
        Ok(d) if d.memory_model == 1 && d.profile_format == 4 && usize::from(d.sector_size) == SECTOR_SIZE
    )
}

/// `$XDG_DATA_HOME/ghub-linux/onboard-backup` (padrão `~/.local/share/...`).
pub fn default_backup_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("ghub-linux/onboard-backup")
}

/// Salva o setor bruto em `dir/<tag>-sector<hhhh>-<unix>.bin`.
pub fn save_backup(dir: &Path, tag: &str, p: &ActiveProfile) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = dir.join(format!("{tag}-sector{:04x}-{secs}.bin", p.sector));
    std::fs::write(&path, p.bytes)?;
    Ok(path)
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
    fn unknown_mode_is_reported_not_fatal() {
        // Modos que o G HUB do Windows grava (ex.: 0x04) não devem derrubar a leitura.
        let led = Led::parse(&[0x7f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(led, Led::Other { mode: 0x7f });
        assert_eq!(led.parts(), ("other", String::new(), 0, 0));
        assert_eq!(led.kind().name(), "other");
        assert!(!LedKind::ALL.contains(&led.kind()), "Other nunca é oferecido como efeito");
        assert!(Led::from_parts("other", "", 0, 0).is_err());
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

    use super::super::onboard_fake::{sample_profile_sector, MemOnboard};
    use crate::device::Device;
    use std::collections::HashMap;

    const FI: u8 = 8;
    const CYCLE: Led = Led::Cycle { period_ms: 8000, intensity: 0 };
    const RED: Led = Led::Fixed { rgb: [0xff, 0, 0] };

    fn dev(mem: MemOnboard) -> Device<MemOnboard> {
        Device::with_features(mem, 1, HashMap::from([(ONBOARD_PROFILES, FI)]))
    }

    #[test]
    fn reads_the_active_profile_through_the_directory() {
        let mut d = dev(MemOnboard::new(FI, &CYCLE));
        let p = read_active_profile(&mut d).unwrap();
        assert_eq!((p.number, p.sector), (1, 1));
        assert_eq!(p.led(), Ok(CYCLE));
        assert_eq!(read_led(&mut d), Ok(CYCLE));
    }

    #[test]
    fn with_led_changes_only_the_led_entries_and_the_crc() {
        let base = ActiveProfile { number: 1, sector: 1, bytes: sample_profile_sector(&CYCLE) };
        let new = base.with_led(&RED);
        assert!(verify(&new.bytes));
        for i in 0..SECTOR_SIZE - 2 {
            let in_led = LED_OFFSETS.iter().any(|o| (*o..*o + LED_LEN).contains(&i));
            if !in_led {
                assert_eq!(new.bytes[i], base.bytes[i], "byte {i:#x} mudou");
            }
        }
        assert_eq!(new.led(), Ok(RED));
    }

    #[test]
    fn set_led_backs_up_writes_verifies_and_reloads() {
        let mut d = dev(MemOnboard::new(FI, &CYCLE));
        let mut saved = vec![];
        set_led(&mut d, &RED, |p| {
            saved.push(p.clone());
            Ok(())
        })
        .unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].led(), Ok(CYCLE));
        let mem = d.into_transport();
        assert_eq!(mem.commits, vec![1]);
        assert_eq!(mem.reloads, vec![1]);
        assert!(verify(&mem.sectors[&1]));
        assert_eq!(ActiveProfile { number: 1, sector: 1, bytes: mem.sectors[&1] }.led(), Ok(RED));
    }

    #[test]
    fn set_led_with_the_same_led_neither_backs_up_nor_writes() {
        let mut d = dev(MemOnboard::new(FI, &CYCLE));
        set_led(&mut d, &CYCLE, |_| panic!("não devia salvar backup")).unwrap();
        let mem = d.into_transport();
        assert!(mem.commits.is_empty() && mem.reloads.is_empty());
    }

    #[test]
    fn refuses_a_sector_with_a_bad_crc_and_writes_nothing() {
        let mut mem = MemOnboard::new(FI, &CYCLE);
        mem.sectors.get_mut(&1).unwrap()[20] ^= 0xff;
        let mut d = dev(mem);
        assert_eq!(set_led(&mut d, &RED, |_| Ok(())), Err(HidppError::BadChecksum));
        assert!(d.into_transport().commits.is_empty());
    }

    #[test]
    fn a_failed_backup_aborts_before_writing() {
        let mut d = dev(MemOnboard::new(FI, &CYCLE));
        let r = set_led(&mut d, &RED, |_| Err(std::io::Error::other("disco cheio")));
        assert!(matches!(r, Err(HidppError::Io(m)) if m.contains("disco cheio")));
        assert!(d.into_transport().commits.is_empty());
    }

    #[test]
    fn a_verify_mismatch_restores_the_original_sector() {
        let mut mem = MemOnboard::new(FI, &CYCLE);
        mem.corrupt_next_commit = true;
        let original = mem.sectors[&1];
        let mut d = dev(mem);
        assert_eq!(set_led(&mut d, &RED, |_| Ok(())), Err(HidppError::VerifyFailed));
        let mem = d.into_transport();
        assert_eq!(mem.commits, vec![1, 1], "gravou e regravou o original");
        assert_eq!(mem.sectors[&1], original);
        assert!(mem.reloads.is_empty());
    }

    #[test]
    fn unsupported_feature_is_reported() {
        let mut d = Device::with_features(MemOnboard::new(FI, &CYCLE), 1, HashMap::new());
        assert_eq!(read_led(&mut d), Err(HidppError::FeatureNotSupported(ONBOARD_PROFILES)));
    }

    #[test]
    fn save_backup_writes_the_raw_sector() {
        let dir = std::env::temp_dir().join(format!("ghub-onboard-bk-{}", std::process::id()));
        let p = ActiveProfile { number: 1, sector: 1, bytes: sample_profile_sector(&CYCLE) };
        let path = save_backup(&dir, "dev1", &p).unwrap();
        assert!(path.file_name().unwrap().to_string_lossy().starts_with("dev1-sector0001-"));
        assert_eq!(std::fs::read(&path).unwrap(), p.bytes.to_vec());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn default_backup_dir_ends_with_the_app_path() {
        assert!(default_backup_dir().ends_with("ghub-linux/onboard-backup"));
    }

    #[test]
    fn a_transport_error_mid_write_still_tries_to_restore_the_original() {
        let mut mem = MemOnboard::new(FI, &CYCLE);
        mem.fail_nth_chunk = Some(3);
        let original = mem.sectors[&1];
        let mut d = dev(mem);
        assert_eq!(set_led(&mut d, &RED, |_| Ok(())), Err(HidppError::Timeout));
        let mem = d.into_transport();
        assert_eq!(mem.commits, vec![1], "uma regravação do original");
        assert_eq!(mem.sectors[&1], original);
    }

    #[test]
    fn a_failed_restore_is_reported_distinctly() {
        let mut mem = MemOnboard::new(FI, &CYCLE);
        mem.corrupt_all_commits = true;
        let mut d = dev(mem);
        assert_eq!(set_led(&mut d, &RED, |_| Ok(())), Err(HidppError::RestoreFailed));
        assert_eq!(d.into_transport().commits, vec![1, 1], "gravou e tentou restaurar");
    }

    #[test]
    fn restore_sector_writes_a_valid_backup_verifies_and_reloads() {
        let mut mem = MemOnboard::new(FI, &CYCLE);
        let good = mem.sectors[&1];
        mem.sectors.get_mut(&1).unwrap()[20] ^= 0xff; // setor corrompido no mouse
        let mut d = dev(mem);
        restore_sector(&mut d, 1, &good).unwrap();
        let mem = d.into_transport();
        assert_eq!(mem.sectors[&1], good);
        assert_eq!(mem.reloads, vec![1]);
    }

    #[test]
    fn restore_sector_refuses_a_backup_with_a_bad_crc() {
        let mem = MemOnboard::new(FI, &CYCLE);
        let mut bad = mem.sectors[&1];
        bad[7] ^= 1;
        let mut d = dev(mem);
        assert_eq!(restore_sector(&mut d, 1, &bad), Err(HidppError::BadChecksum));
        assert!(d.into_transport().commits.is_empty());
    }

    #[test]
    fn supports_led_only_for_the_layout_verified_on_the_g703() {
        let mut d = dev(MemOnboard::new(FI, &CYCLE));
        assert!(supports_led(&mut d));
        for (idx, val) in [(0usize, 2u8), (1, 5), (8, 0xfe)] {
            let mut mem = MemOnboard::new(FI, &CYCLE);
            mem.description[idx] = val;
            assert!(!supports_led(&mut dev(mem)), "byte {idx} = {val}");
        }
    }
}
