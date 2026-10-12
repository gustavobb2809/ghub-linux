use std::fs;
use std::path::PathBuf;

use crate::error::Result;

const LOGITECH_VENDOR: u16 = 0x046d;
const SYSFS_HIDRAW: &str = "/sys/class/hidraw";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidrawInfo {
    pub path: PathBuf,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
}

/// Lê `HID_ID=bus:vendor:produto` e `HID_NAME=` do uevent de um dispositivo HID.
pub fn parse_uevent(text: &str) -> Option<(u16, u16, String)> {
    let mut ids = None;
    let mut name = String::new();
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("HID_ID=") {
            let mut parts = v.split(':');
            let _bus = parts.next()?;
            let vendor = u32::from_str_radix(parts.next()?, 16).ok()?;
            let product = u32::from_str_radix(parts.next()?, 16).ok()?;
            ids = Some((u16::try_from(vendor).ok()?, u16::try_from(product).ok()?));
        } else if let Some(v) = line.strip_prefix("HID_NAME=") {
            name = v.to_string();
        }
    }
    ids.map(|(v, p)| (v, p, name))
}

/// A interface HID++ expõe uma página de uso do fabricante (0xFF00 ou 0xFF43)
/// e o Report ID 0x11 (relatório longo).
pub fn has_hidpp_collection(descriptor: &[u8]) -> bool {
    let has = |needle: &[u8]| descriptor.windows(needle.len()).any(|w| w == needle);
    (has(&[0x06, 0x00, 0xFF]) || has(&[0x06, 0x43, 0xFF])) && has(&[0x85, 0x11])
}

/// Lista as interfaces hidraw da Logitech que falam HID++, ordenadas por caminho.
pub fn list_logitech() -> Result<Vec<HidrawInfo>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(SYSFS_HIDRAW)? {
        let entry = entry?;
        let node = entry.file_name().to_string_lossy().into_owned();
        let device_dir = entry.path().join("device");
        let Ok(uevent) = fs::read_to_string(device_dir.join("uevent")) else {
            continue;
        };
        let Some((vendor_id, product_id, name)) = parse_uevent(&uevent) else {
            continue;
        };
        if vendor_id != LOGITECH_VENDOR {
            continue;
        }
        let Ok(descriptor) = fs::read(device_dir.join("report_descriptor")) else {
            continue;
        };
        if !has_hidpp_collection(&descriptor) {
            continue;
        }
        out.push(HidrawInfo {
            path: PathBuf::from("/dev").join(node),
            vendor_id,
            product_id,
            name,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UEVENT: &str = "DRIVER=hid-generic\n\
HID_ID=0003:0000046D:0000C539\n\
HID_NAME=Logitech USB Receiver\n\
HID_PHYS=usb-0000:00:14.0-2/input2\n";

    #[test]
    fn parses_uevent() {
        assert_eq!(
            parse_uevent(UEVENT),
            Some((0x046d, 0xc539, "Logitech USB Receiver".to_string()))
        );
    }

    #[test]
    fn uevent_without_hid_id_is_none() {
        assert_eq!(parse_uevent("DRIVER=hid-generic\n"), None);
    }

    #[test]
    fn uevent_with_garbage_id_is_none() {
        assert_eq!(parse_uevent("HID_ID=zzzz\n"), None);
    }

    #[test]
    fn detects_hidpp_descriptor() {
        // Usage Page 0xFF00, Report ID 0x11
        let d = [
            0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x11, 0x75, 0x08, 0x95, 0x13, 0xC0,
        ];
        assert!(has_hidpp_collection(&d));
    }

    #[test]
    fn rejects_keyboard_descriptor() {
        let d = [0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, 0x05, 0x07, 0xC0];
        assert!(!has_hidpp_collection(&d));
    }

    #[test]
    fn vendor_page_without_report_id_11_is_rejected() {
        let d = [0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x10, 0xC0];
        assert!(!has_hidpp_collection(&d));
    }
}
