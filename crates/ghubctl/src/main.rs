use std::error::Error;
use std::time::Duration;

use clap::{Parser, Subcommand};
use hidpp::device::{Device, DEFAULT_TIMEOUT};
use hidpp::enumerate::list_logitech;
use hidpp::features::{self, battery, dpi, onboard, report_rate};
use hidpp::hidraw::HidrawTransport;
use hidpp::transport::Transport;

#[derive(Parser)]
#[command(name = "ghubctl", about = "Diagnóstico e controle de dispositivos Logitech")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Lista as interfaces hidraw HID++ da Logitech
    List,
    /// Descobre dispositivos e features (somente leitura)
    Probe,
    /// Lê ou grava o LED do perfil onboard ativo (pare o ghubd antes: `systemctl --user stop ghubd`)
    Led {
        #[command(subcommand)]
        action: LedAction,
    },
}

#[derive(Subcommand)]
enum LedAction {
    /// Mostra o efeito gravado no perfil ativo
    Get,
    /// Grava um efeito (sem --yes só mostra o que faria)
    Set {
        /// off | fixed | cycle | breathing
        effect: String,
        /// #RRGGBB (fixed e breathing)
        #[arg(long, default_value = "")]
        color: String,
        /// Período em ms (cycle e breathing)
        #[arg(long, default_value_t = 4000)]
        period: u32,
        /// 0 a 100; 0 = 100% (cycle e breathing)
        #[arg(long, default_value_t = 0)]
        intensity: u32,
        /// Confirma a gravação na flash do mouse
        #[arg(long)]
        yes: bool,
    },
    /// Regrava uma cópia de segurança (`...-sectorHHHH-....bin`) no setor do perfil
    Restore {
        /// Arquivo salvo em ~/.local/share/ghub-linux/onboard-backup/
        file: std::path::PathBuf,
        /// Confirma a gravação na flash do mouse
        #[arg(long)]
        yes: bool,
    },
}

/// Setor de uma cópia de segurança, lido do nome `...-sectorHHHH-...`.
fn backup_sector(file: &std::path::Path) -> Option<u16> {
    let name = file.file_name()?.to_str()?;
    let rest = name.split("-sector").nth(1)?;
    u16::from_str_radix(rest.get(..4)?, 16).ok()
}

/// Índices tentados em cada interface: slots do receptor (1-6), o próprio receptor (0xFF)
/// e um dispositivo conectado direto (0x00).
const CANDIDATE_INDICES: [u8; 8] = [1, 2, 3, 4, 5, 6, 0xFF, 0x00];
const PING_TIMEOUT: Duration = Duration::from_millis(400);

fn main() -> Result<(), Box<dyn Error>> {
    match Cli::parse().command {
        Command::List => list(),
        Command::Probe => probe(),
        Command::Led { action } => led(action),
    }
}

fn open_onboard() -> Result<Device<HidrawTransport>, Box<dyn Error>> {
    for info in list_logitech()? {
        let Ok(mut transport) = HidrawTransport::open(&info.path) else { continue };
        for index in CANDIDATE_INDICES {
            let mut dev = Device::new(transport, index);
            dev.set_timeout(PING_TIMEOUT);
            let found = dev.ping().is_ok() && {
                dev.set_timeout(DEFAULT_TIMEOUT);
                dev.discover().is_ok()
            } && dev.feature_index(onboard::ONBOARD_PROFILES).is_ok();
            if found {
                dev.set_timeout(DEFAULT_TIMEOUT);
                return Ok(dev);
            }
            transport = dev.into_transport();
        }
    }
    Err("nenhum dispositivo com perfis onboard (0x8100) encontrado".into())
}

fn led(action: LedAction) -> Result<(), Box<dyn Error>> {
    match action {
        LedAction::Get => {
            let mut dev = open_onboard()?;
            let (effect, color, period, intensity) = onboard::read_led(&mut dev)?.parts();
            println!("effect={effect} color={color} period_ms={period} intensity={intensity}");
            Ok(())
        }
        LedAction::Restore { file, yes } => {
            let sector = backup_sector(&file)
                .ok_or("o nome do arquivo deve conter -sectorHHHH- (como salvo pelo ghubctl/ghubd)")?;
            let raw = std::fs::read(&file)?;
            let bytes: onboard::Sector = raw
                .as_slice()
                .try_into()
                .map_err(|_| format!("a cópia deve ter {} bytes (tem {})", onboard::SECTOR_SIZE, raw.len()))?;
            if !onboard::verify(&bytes) {
                return Err("a cópia de segurança tem CRC inválido; não será gravada".into());
            }
            if !yes {
                println!("Regravaria {} no setor {sector:#06x}. Repita com --yes para gravar na flash do mouse.", file.display());
                return Ok(());
            }
            let mut dev = open_onboard()?;
            onboard::restore_sector(&mut dev, sector, &bytes)?;
            println!("restaurado o setor {sector:#06x}");
            Ok(())
        }
        LedAction::Set { effect, color, period, intensity, yes } => {
            let led = onboard::Led::from_parts(&effect, &color, period, intensity)?;
            if !yes {
                println!("Gravaria {led:?} no perfil ativo. Repita com --yes para gravar na flash do mouse.");
                return Ok(());
            }
            let mut dev = open_onboard()?;
            let dir = onboard::default_backup_dir();
            onboard::set_led(&mut dev, &led, |p| {
                let path = onboard::save_backup(&dir, "ghubctl", p)?;
                println!("cópia de segurança: {}", path.display());
                Ok(())
            })?;
            println!("gravado: {led:?}");
            Ok(())
        }
    }
}

fn list() -> Result<(), Box<dyn Error>> {
    let interfaces = list_logitech()?;
    if interfaces.is_empty() {
        println!("Nenhuma interface HID++ da Logitech encontrada.");
    }
    for i in interfaces {
        println!(
            "{}  {:04x}:{:04x}  {}",
            i.path.display(),
            i.vendor_id,
            i.product_id,
            i.name
        );
    }
    Ok(())
}

fn probe() -> Result<(), Box<dyn Error>> {
    let interfaces = list_logitech()?;
    if interfaces.is_empty() {
        println!("Nenhuma interface HID++ da Logitech encontrada.");
        return Ok(());
    }
    for info in interfaces {
        println!(
            "{}  {:04x}:{:04x}  {}",
            info.path.display(),
            info.vendor_id,
            info.product_id,
            info.name
        );
        let mut transport = match HidrawTransport::open(&info.path) {
            Ok(t) => t,
            Err(e) => {
                println!("  não foi possível abrir: {e}");
                println!("  (sem permissão? veja docs/udev/70-ghub-linux.rules)");
                continue;
            }
        };
        for index in CANDIDATE_INDICES {
            let mut dev = Device::new(transport, index);
            dev.set_timeout(PING_TIMEOUT);
            match dev.ping() {
                Ok((major, minor)) => {
                    println!("  índice 0x{index:02x}: HID++ {major}.{minor}");
                    dev.set_timeout(DEFAULT_TIMEOUT);
                    match dev.discover() {
                        Ok(()) => describe(&mut dev),
                        Err(e) => println!("    falha ao listar features: {e}"),
                    }
                }
                Err(e) => println!("  índice 0x{index:02x}: {e}"),
            }
            transport = dev.into_transport();
        }
    }
    Ok(())
}

/// Imprime features e valores atuais. Só leituras; nunca escreve no dispositivo.
fn describe<T: Transport>(dev: &mut Device<T>) {
    println!("    features:");
    for (id, index) in dev.features() {
        println!(
            "      0x{id:04x} (índice {index}) {}",
            features::name(id).unwrap_or("?")
        );
    }
    match battery::read(dev) {
        Ok(b) => println!("    bateria: {b:?}"),
        Err(e) => println!("    bateria: {e}"),
    }
    match dpi::current(dev, 0) {
        Ok((now, default)) => println!("    DPI atual: {now} (padrão {default})"),
        Err(e) => println!("    DPI atual: {e}"),
    }
    match dpi::supported(dev, 0) {
        Ok(s) => println!("    DPI suportado: {s:?}"),
        Err(e) => println!("    DPI suportado: {e}"),
    }
    match report_rate::current_hz(dev) {
        Ok(hz) => println!("    polling atual: {hz} Hz"),
        Err(e) => println!("    polling atual: {e}"),
    }
    match report_rate::supported_hz(dev) {
        Ok(v) => println!("    polling suportado: {v:?} Hz"),
        Err(e) => println!("    polling suportado: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backup_sector_comes_from_the_file_name() {
        let p = std::path::Path::new("/x/ghubctl-sector0001-1791898388.bin");
        assert_eq!(backup_sector(p), Some(1));
        assert_eq!(backup_sector(std::path::Path::new("dev1-sector01ab-9.bin")), Some(0x01ab));
        assert_eq!(backup_sector(std::path::Path::new("sem-setor.bin")), None);
        assert_eq!(backup_sector(std::path::Path::new("x-sectorzz-1.bin")), None);
    }
}
