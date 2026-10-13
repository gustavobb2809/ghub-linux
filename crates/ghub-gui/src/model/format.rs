use ghub_ipc::{STATE_CHARGING, STATE_FULL};

pub const LOW_BATTERY_PERCENT: i32 = 15;

pub fn battery_text(percent: i32, state: &str) -> String {
    if percent < 0 { return "—".into(); }
    match state {
        STATE_CHARGING => format!("{percent}% · carregando"),
        STATE_FULL => format!("{percent}% · cheia"),
        _ => format!("{percent}%"),
    }
}
pub fn battery_is_low(percent: i32) -> bool { (0..=LOW_BATTERY_PERCENT).contains(&percent) }
pub fn dpi_text(dpi: u32) -> String { if dpi == 0 { "lendo…".into() } else { format!("{dpi} DPI") } }
pub fn polling_text(hz: u32) -> String { if hz == 0 { "—".into() } else { format!("{hz} Hz") } }
pub fn json_u32s(v: &[u32]) -> String { serde_json::to_string(v).unwrap_or_else(|_| "[]".into()) }
pub fn json_strings(v: &[String]) -> String { serde_json::to_string(v).unwrap_or_else(|_| "[]".into()) }

#[cfg(test)]
mod tests {
    use super::*;
    use ghub_ipc::{STATE_DISCHARGING, STATE_UNKNOWN};
    #[test]
    fn battery_variants() {
        assert_eq!(battery_text(-1, STATE_UNKNOWN), "—");
        assert_eq!(battery_text(91, STATE_DISCHARGING), "91%");
        assert_eq!(battery_text(50, STATE_CHARGING), "50% · carregando");
        assert_eq!(battery_text(100, STATE_FULL), "100% · cheia");
        assert!(battery_is_low(15) && battery_is_low(0));
        assert!(!battery_is_low(16) && !battery_is_low(-1));
    }
    #[test]
    fn zero_dpi_never_prints_zero() {
        assert_eq!(dpi_text(0), "lendo…");
        assert_eq!(dpi_text(1200), "1200 DPI");
        assert_eq!(polling_text(1000), "1000 Hz");
        assert_eq!(polling_text(0), "—");
    }
    #[test]
    fn json_lists() {
        assert_eq!(json_u32s(&[400, 800]), "[400,800]");
        assert_eq!(json_strings(&["a\"b".to_string()]), "[\"a\\\"b\"]");
        assert_eq!(json_u32s(&[]), "[]");
    }
}
