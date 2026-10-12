use std::fs;
use std::path::{Path, PathBuf};

use ghub_ipc::{STATE_CHARGING, STATE_DISCHARGING, STATE_FULL, STATE_UNKNOWN};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeStatus {
    Unknown,
    Discharging,
    Charging,
    Full,
}

impl ChargeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            ChargeStatus::Unknown => STATE_UNKNOWN,
            ChargeStatus::Discharging => STATE_DISCHARGING,
            ChargeStatus::Charging => STATE_CHARGING,
            ChargeStatus::Full => STATE_FULL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatteryInfo {
    pub percent: u8,
    pub status: ChargeStatus,
    pub voltage_mv: Option<u32>,
}

/// Procura baterias `power_supply` de dispositivos HID abaixo de `hid_dir`
/// (e do próprio `hid_dir`). Só considera entradas com `type` igual a `Battery`.
pub fn find_batteries(hid_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![hid_dir.join("power_supply")];
    if let Ok(children) = fs::read_dir(hid_dir) {
        for child in children.flatten() {
            candidates.push(child.path().join("power_supply"));
        }
    }
    let mut out = Vec::new();
    for ps in candidates {
        let Ok(entries) = fs::read_dir(&ps) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            let is_battery = fs::read_to_string(p.join("type"))
                .map(|t| t.trim() == "Battery")
                .unwrap_or(false);
            if is_battery {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

pub fn read_battery(dir: &Path) -> Option<BatteryInfo> {
    let percent: u8 = fs::read_to_string(dir.join("capacity"))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if percent > 100 {
        return None;
    }
    let status = match fs::read_to_string(dir.join("status"))
        .ok()
        .as_deref()
        .map(str::trim)
    {
        Some("Charging") => ChargeStatus::Charging,
        Some("Discharging") => ChargeStatus::Discharging,
        Some("Full") => ChargeStatus::Full,
        _ => ChargeStatus::Unknown,
    };
    let voltage_mv = fs::read_to_string(dir.join("voltage_now"))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .map(|uv| (uv / 1000) as u32);
    Some(BatteryInfo {
        percent,
        status,
        voltage_mv,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_battery(dir: &Path, kind: &str, capacity: &str, status: &str, voltage: Option<&str>) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("type"), format!("{kind}\n")).unwrap();
        fs::write(dir.join("capacity"), format!("{capacity}\n")).unwrap();
        fs::write(dir.join("status"), format!("{status}\n")).unwrap();
        if let Some(v) = voltage {
            fs::write(dir.join("voltage_now"), format!("{v}\n")).unwrap();
        }
    }

    #[test]
    fn reads_the_real_g703_values() {
        let t = tempfile::tempdir().unwrap();
        write_battery(t.path(), "Battery", "91", "Discharging", Some("4083000"));
        assert_eq!(
            read_battery(t.path()),
            Some(BatteryInfo {
                percent: 91,
                status: ChargeStatus::Discharging,
                voltage_mv: Some(4083),
            })
        );
    }

    #[test]
    fn maps_statuses_and_tolerates_missing_voltage() {
        let t = tempfile::tempdir().unwrap();
        for (raw, expected) in [
            ("Charging", ChargeStatus::Charging),
            ("Full", ChargeStatus::Full),
            ("Not charging", ChargeStatus::Unknown),
            ("whatever", ChargeStatus::Unknown),
        ] {
            write_battery(t.path(), "Battery", "50", raw, None);
            let b = read_battery(t.path()).unwrap();
            assert_eq!(b.status, expected, "{raw}");
            assert_eq!(b.voltage_mv, None);
        }
    }

    #[test]
    fn rejects_bad_capacity() {
        let t = tempfile::tempdir().unwrap();
        for bad in ["abc", "101", "-3", ""] {
            write_battery(t.path(), "Battery", bad, "Discharging", None);
            assert_eq!(read_battery(t.path()), None, "{bad}");
        }
        assert_eq!(read_battery(&t.path().join("nao-existe")), None);
    }

    #[test]
    fn status_strings_match_the_ipc_contract() {
        assert_eq!(ChargeStatus::Unknown.as_str(), STATE_UNKNOWN);
        assert_eq!(ChargeStatus::Discharging.as_str(), STATE_DISCHARGING);
        assert_eq!(ChargeStatus::Charging.as_str(), STATE_CHARGING);
        assert_eq!(ChargeStatus::Full.as_str(), STATE_FULL);
    }

    #[test]
    fn finds_batteries_under_child_hid_devices_only_of_type_battery() {
        let t = tempfile::tempdir().unwrap();
        let hid = t.path().join("0003:046D:C539.0005");
        let child = hid.join("0003:046D:4086.0009/power_supply");
        write_battery(&child.join("hidpp_battery_0"), "Battery", "91", "Discharging", None);
        write_battery(&child.join("ac"), "Mains", "1", "Unknown", None);
        let found = find_batteries(&hid);
        assert_eq!(found, vec![child.join("hidpp_battery_0")]);
    }

    #[test]
    fn find_batteries_on_missing_dir_is_empty() {
        let t = tempfile::tempdir().unwrap();
        assert!(find_batteries(&t.path().join("nada")).is_empty());
    }
}
