//! Validação do formulário de iluminação antes de enviar ao daemon (que valida de novo).

use ghub_ipc::LedSetting;

pub const MIN_PERIOD_MS: i32 = 100;
pub const MAX_PERIOD_MS: i32 = 60_000;

fn normalize_color(color: &str) -> Result<String, String> {
    let hex = color.strip_prefix('#').unwrap_or("");
    if hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(format!("#{}", hex.to_ascii_uppercase()))
    } else {
        Err("Cor inválida: use o formato #RRGGBB.".into())
    }
}

pub fn validate(
    effect: &str,
    color: &str,
    period_ms: i32,
    intensity: i32,
    supported: &[String],
) -> Result<LedSetting, String> {
    if !supported.iter().any(|e| e == effect) {
        return Err("Este efeito não é suportado pelo mouse.".into());
    }
    let wants_color = matches!(effect, "fixed" | "breathing");
    let wants_motion = matches!(effect, "cycle" | "breathing");
    let color = if wants_color { normalize_color(color)? } else { String::new() };
    let (period_ms, intensity) = if wants_motion {
        if !(MIN_PERIOD_MS..=MAX_PERIOD_MS).contains(&period_ms) {
            return Err(format!("O período deve ficar entre {MIN_PERIOD_MS} e {MAX_PERIOD_MS} ms."));
        }
        if !(0..=100).contains(&intensity) {
            return Err("A intensidade deve ficar entre 0 e 100.".into());
        }
        (period_ms as u32, intensity as u32)
    } else {
        (0, 0)
    };
    Ok(LedSetting { effect: effect.into(), color, period_ms, intensity })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<String> {
        ["off", "fixed", "cycle", "breathing"].map(String::from).to_vec()
    }

    #[test]
    fn accepts_each_effect_with_its_fields() {
        assert_eq!(validate("off", "", 0, 0, &all()).unwrap().effect, "off");
        let f = validate("fixed", "#fe8019", 0, 0, &all()).unwrap();
        assert_eq!((f.effect.as_str(), f.color.as_str()), ("fixed", "#FE8019"));
        assert_eq!(validate("cycle", "", 8000, 0, &all()).unwrap().period_ms, 8000);
        assert_eq!(validate("breathing", "#00ff00", 4000, 80, &all()).unwrap().intensity, 80);
    }

    #[test]
    fn rejects_bad_fields_with_a_readable_message() {
        for (e, c, p, i) in [
            ("sparkle", "", 0, 0),
            ("fixed", "", 0, 0),
            ("fixed", "#zzzzzz", 0, 0),
            ("cycle", "", 50, 0),
            ("cycle", "", 70000, 0),
            ("breathing", "#ff0000", 4000, 101),
            ("breathing", "#ff0000", 4000, -1),
        ] {
            let err = validate(e, c, p, i, &all()).unwrap_err();
            assert!(!err.is_empty(), "{e} {c} {p} {i}");
        }
    }

    #[test]
    fn rejects_an_effect_the_device_does_not_list() {
        let only_fixed = vec!["fixed".to_string()];
        assert!(validate("off", "", 0, 0, &only_fixed).is_err());
    }
}
