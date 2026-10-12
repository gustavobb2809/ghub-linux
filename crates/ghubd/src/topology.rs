use std::fs;
use std::path::{Path, PathBuf};

use hidpp::enumerate::{has_hidpp_collection, parse_uevent};

const LOGITECH_VENDOR: u16 = 0x046d;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidNode {
    /// Caminho do nó, por exemplo `/dev/hidraw4`.
    pub devnode: PathBuf,
    /// Diretório do dispositivo HID no sysfs (canônico).
    pub hid_dir: PathBuf,
    pub vendor_id: u16,
    pub product_id: u16,
    pub name: String,
}

/// Nome de dispositivo HID no sysfs, por exemplo `0003:046D:C539.0005`.
fn is_hid_name(name: &str) -> bool {
    name.len() == 19
        && name.char_indices().all(|(i, c)| match i {
            4 | 9 => c == ':',
            14 => c == '.',
            _ => c.is_ascii_hexdigit(),
        })
}

/// Nós hidraw HID++ da Logitech que **não** são filhos de outro dispositivo HID.
///
/// O receptor Lightspeed aparece como nó de topo; o mouse pareado aparece como nó filho
/// do `hid-logitech-dj` e é ignorado aqui (o daemon fala com o mouse pelo nó do receptor).
pub fn top_level_nodes(sysfs: &Path, dev: &Path) -> std::io::Result<Vec<HidNode>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(sysfs.join("class/hidraw"))? {
        let entry = entry?;
        let node = entry.file_name().to_string_lossy().into_owned();
        let Ok(hid_dir) = fs::canonicalize(entry.path().join("device")) else {
            continue;
        };
        let parent_is_hid = hid_dir
            .parent()
            .and_then(Path::file_name)
            .map(|n| is_hid_name(&n.to_string_lossy()))
            .unwrap_or(false);
        if parent_is_hid {
            continue;
        }
        let Ok(uevent) = fs::read_to_string(hid_dir.join("uevent")) else {
            continue;
        };
        let Some((vendor_id, product_id, name)) = parse_uevent(&uevent) else {
            continue;
        };
        if vendor_id != LOGITECH_VENDOR {
            continue;
        }
        let Ok(descriptor) = fs::read(hid_dir.join("report_descriptor")) else {
            continue;
        };
        if !has_hidpp_collection(&descriptor) {
            continue;
        }
        out.push(HidNode {
            devnode: dev.join(node),
            hid_dir,
            vendor_id,
            product_id,
            name,
        });
    }
    out.sort_by(|a, b| a.devnode.cmp(&b.devnode));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const HIDPP_DESC: [u8; 9] = [0x06, 0x00, 0xFF, 0x09, 0x01, 0xA1, 0x01, 0x85, 0x11];
    const KEYBOARD_DESC: [u8; 8] = [0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01];

    fn make_hid(dir: &Path, vid: &str, pid: &str, name: &str, desc: &[u8]) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("uevent"),
            format!("HID_ID=0003:0000{vid}:0000{pid}\nHID_NAME={name}\n"),
        )
        .unwrap();
        fs::write(dir.join("report_descriptor"), desc).unwrap();
    }

    fn link_hidraw(root: &Path, node: &str, hid_dir: &Path) {
        let d = root.join("class/hidraw").join(node);
        fs::create_dir_all(&d).unwrap();
        symlink(hid_dir, d.join("device")).unwrap();
    }

    /// Reproduz a árvore real: receptor (interface HID++ e uma de teclado) e o mouse como filho.
    fn fixture() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let usb = t.path().join("devices/usb/3-4");
        let hidpp = usb.join("3-4:1.2/0003:046D:C539.0005");
        let kbd = usb.join("3-4:1.1/0003:046D:C539.0003");
        let child = hidpp.join("0003:046D:4086.0009");
        make_hid(&hidpp, "046D", "C539", "Logitech USB Receiver", &HIDPP_DESC);
        make_hid(&kbd, "046D", "C539", "Logitech USB Receiver", &KEYBOARD_DESC);
        make_hid(&child, "046D", "4086", "Logitech G703 LS", &HIDPP_DESC);
        link_hidraw(t.path(), "hidraw4", &hidpp);
        link_hidraw(t.path(), "hidraw3", &kbd);
        link_hidraw(t.path(), "hidraw8", &child);
        t
    }

    #[test]
    fn keeps_only_top_level_hidpp_nodes() {
        let t = fixture();
        let nodes = top_level_nodes(t.path(), Path::new("/dev")).unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].devnode, PathBuf::from("/dev/hidraw4"));
        assert_eq!(nodes[0].vendor_id, 0x046d);
        assert_eq!(nodes[0].product_id, 0xc539);
        assert_eq!(nodes[0].name, "Logitech USB Receiver");
        assert!(nodes[0].hid_dir.ends_with("0003:046D:C539.0005"));
    }

    #[test]
    fn ignores_non_logitech_nodes() {
        let t = fixture();
        let other = t.path().join("devices/usb/1-1/1-1:1.0/0003:1234:5678.0001");
        make_hid(&other, "1234", "5678", "Outro", &HIDPP_DESC);
        link_hidraw(t.path(), "hidraw1", &other);
        let nodes = top_level_nodes(t.path(), Path::new("/dev")).unwrap();
        assert_eq!(nodes.len(), 1);
    }

    #[test]
    fn missing_class_dir_is_an_error() {
        let t = tempfile::tempdir().unwrap();
        assert!(top_level_nodes(t.path(), Path::new("/dev")).is_err());
    }

    #[test]
    fn broken_symlink_is_skipped() {
        let t = fixture();
        let d = t.path().join("class/hidraw/hidraw9");
        fs::create_dir_all(&d).unwrap();
        symlink(t.path().join("nao/existe"), d.join("device")).unwrap();
        let nodes = top_level_nodes(t.path(), Path::new("/dev")).unwrap();
        assert_eq!(nodes.len(), 1);
    }
}
