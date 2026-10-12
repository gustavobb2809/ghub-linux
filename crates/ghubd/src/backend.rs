use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use hidpp::device::{Device, DEFAULT_TIMEOUT};
use hidpp::features::device_name;
use hidpp::hidraw::HidrawTransport;

use crate::battery_sysfs::{find_batteries, read_battery, BatteryInfo};
use crate::mouse::{HidppMouse, Mouse};
use crate::topology::{top_level_nodes, HidNode};

/// Tempo máximo esperando a resposta de um slot do receptor durante a varredura.
const PROBE_TIMEOUT: Duration = Duration::from_millis(300);
/// Slots de dispositivo de um receptor Lightspeed/Unifying.
const SLOTS: std::ops::RangeInclusive<u8> = 1..=6;

pub struct Found {
    pub id: String,
    pub name: String,
    pub mouse: Box<dyn Mouse>,
}

/// Fonte de dispositivos e de bateria. A implementação real usa sysfs e hidraw.
pub trait Backend: Send {
    /// Abre todos os mouses que respondem agora, sem tocar nos ids de `skip`
    /// (dispositivos já online, que têm o próprio descritor aberto).
    fn scan(&mut self, skip: &[String]) -> Vec<Found>;
    fn battery(&mut self, id: &str) -> Option<BatteryInfo>;
}

/// Ids que concorrem pela bateria de um nó: os encontrados nesta varredura e os pulados
/// (online) cujo id começa com o prefixo `{vid:04x}{pid:04x}-` do nó.
pub fn battery_candidates(found_ids: &[String], skip: &[String], node_prefix: &str) -> Vec<String> {
    let mut ids = found_ids.to_vec();
    for id in skip {
        if id.starts_with(node_prefix) && !ids.contains(id) {
            ids.push(id.clone());
        }
    }
    ids
}

/// Associa a única bateria de um receptor ao único dispositivo encontrado nele.
pub fn pairing(found_ids: &[String], batteries: &[PathBuf]) -> Option<(String, PathBuf)> {
    match (found_ids, batteries) {
        ([id], [path]) => Some((id.clone(), path.clone())),
        _ => None,
    }
}

pub struct SysBackend {
    sysfs: PathBuf,
    dev: PathBuf,
    batteries: HashMap<String, PathBuf>,
}

impl SysBackend {
    pub fn new() -> Self {
        Self::with_roots(Path::new("/sys"), Path::new("/dev"))
    }

    pub fn with_roots(sysfs: &Path, dev: &Path) -> Self {
        SysBackend {
            sysfs: sysfs.to_path_buf(),
            dev: dev.to_path_buf(),
            batteries: HashMap::new(),
        }
    }
}

impl Default for SysBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn node_prefix(node: &HidNode) -> String {
    format!("{:04x}{:04x}-", node.vendor_id, node.product_id)
}

fn probe_slot(node: &HidNode, slot: u8, skip: &[String]) -> Option<Found> {
    let id = format!("{}{slot}", node_prefix(node));
    if skip.contains(&id) {
        // Já online: um ping aqui deixaria a resposta na fila do descritor dele.
        return None;
    }
    let transport = HidrawTransport::open(&node.devnode).ok()?;
    let mut dev = Device::new(transport, slot);
    dev.set_timeout(PROBE_TIMEOUT);
    match dev.ping() {
        Ok((major, _)) if major >= 2 => {}
        _ => return None,
    }
    dev.set_timeout(DEFAULT_TIMEOUT);
    dev.discover().ok()?;
    let name = device_name::read(&mut dev).unwrap_or_else(|_| node.name.clone());
    Some(Found {
        id,
        name,
        mouse: Box::new(HidppMouse::new(dev)),
    })
}

impl Backend for SysBackend {
    fn scan(&mut self, skip: &[String]) -> Vec<Found> {
        let nodes = match top_level_nodes(&self.sysfs, &self.dev) {
            Ok(nodes) => nodes,
            Err(e) => {
                eprintln!("ghubd: não foi possível listar os nós hidraw: {e}");
                return Vec::new();
            }
        };
        let mut all = Vec::new();
        for node in nodes {
            if let Err(e) = HidrawTransport::open(&node.devnode) {
                eprintln!("ghubd: sem acesso a {} ({e}); veja docs/udev", node.devnode.display());
                continue;
            }
            let found: Vec<Found> = SLOTS.filter_map(|slot| probe_slot(&node, slot, skip)).collect();
            let ids: Vec<String> = found.iter().map(|f| f.id.clone()).collect();
            // O mapeamento anterior é mantido; só é trocado quando o pareamento é inequívoco.
            let candidates = battery_candidates(&ids, skip, &node_prefix(&node));
            if let Some((id, path)) = pairing(&candidates, &find_batteries(&node.hid_dir)) {
                self.batteries.insert(id, path);
            }
            all.extend(found);
        }
        all
    }

    fn battery(&mut self, id: &str) -> Option<BatteryInfo> {
        self.batteries.get(id).and_then(|p| read_battery(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_only_when_exactly_one_device_and_one_battery() {
        let ids = vec!["046dc539-1".to_string()];
        let bat = vec![PathBuf::from("/sys/x/hidpp_battery_0")];
        assert_eq!(
            pairing(&ids, &bat),
            Some(("046dc539-1".to_string(), PathBuf::from("/sys/x/hidpp_battery_0")))
        );
        assert_eq!(pairing(&ids, &[]), None);
        assert_eq!(pairing(&[], &bat), None);
        let two_ids = vec!["a".to_string(), "b".to_string()];
        assert_eq!(pairing(&two_ids, &bat), None);
        let two_bats = vec![PathBuf::from("/a"), PathBuf::from("/b")];
        assert_eq!(pairing(&ids, &two_bats), None);
    }

    #[test]
    fn battery_candidates_include_skipped_ids_of_the_same_node() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        // Mouse online (pulado) no receptor: continua sendo o único candidato.
        assert_eq!(
            battery_candidates(&[], &s(&["046dc539-1"]), "046dc539-"),
            s(&["046dc539-1"])
        );
        // Pulados de outro nó não entram.
        assert_eq!(
            battery_candidates(&s(&["046dc539-2"]), &s(&["046dc547-1"]), "046dc539-"),
            s(&["046dc539-2"])
        );
        // Encontrado + pulado no mesmo nó: dois candidatos, sem pareamento.
        let both = battery_candidates(&s(&["046dc539-2"]), &s(&["046dc539-1"]), "046dc539-");
        assert_eq!(both.len(), 2);
        assert_eq!(pairing(&both, &[PathBuf::from("/b")]), None);
        assert!(battery_candidates(&[], &[], "046dc539-").is_empty());
    }
}
