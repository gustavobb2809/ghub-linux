pub mod battery;
pub mod dpi;
pub mod report_rate;

/// Nome legível de features HID++ conhecidas (para diagnóstico).
pub fn name(id: u16) -> Option<&'static str> {
    Some(match id {
        0x0000 => "IRoot",
        0x0001 => "IFeatureSet",
        0x0003 => "DeviceFwVersion",
        0x0005 => "DeviceName",
        0x1000 => "BatteryStatus",
        0x1004 => "UnifiedBattery",
        0x1B04 => "ReprogControlsV4",
        0x2201 => "AdjustableDpi",
        0x8060 => "ReportRate",
        0x8071 => "RgbEffects",
        0x8100 => "OnboardProfiles",
        _ => return None,
    })
}
