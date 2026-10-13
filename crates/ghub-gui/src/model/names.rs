pub const MAX_NAME_CHARS: usize = 40;

pub fn validate_profile_name(raw: &str, existing: &[String]) -> Result<String, String> {
    let name = raw.trim();
    if name.is_empty() { return Err("Digite um nome para o perfil.".into()); }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("O nome pode ter no máximo {MAX_NAME_CHARS} caracteres."));
    }
    if existing.iter().any(|e| e.trim().to_lowercase() == name.to_lowercase()) {
        return Err("Já existe um perfil com esse nome.".into());
    }
    Ok(name.to_string())
}

/// O daemon recusa apagar o perfil ativo e o último; a GUI nem oferece.
pub fn can_delete(profiles: &[String], active: &str, name: &str) -> bool {
    profiles.len() > 1 && name != active && profiles.iter().any(|p| p == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }
    #[test]
    fn names_are_trimmed_and_checked() {
        assert_eq!(validate_profile_name("  Jogo  ", &p(&["Padrão"])).unwrap(), "Jogo");
        assert!(validate_profile_name("   ", &p(&[])).is_err());
        assert!(validate_profile_name("", &p(&[])).is_err());
        assert!(validate_profile_name(&"x".repeat(41), &p(&[])).is_err());
        assert!(validate_profile_name(&"é".repeat(40), &p(&[])).is_ok());
        assert!(validate_profile_name("padrão", &p(&["Padrão"])).is_err());
    }
    #[test]
    fn delete_rules() {
        let all = p(&["A", "B"]);
        assert!(can_delete(&all, "A", "B"));
        assert!(!can_delete(&all, "A", "A"));
        assert!(!can_delete(&p(&["A"]), "A", "A"));
        assert!(!can_delete(&all, "A", "Z"));
    }
}
