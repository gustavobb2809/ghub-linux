use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use hidpp::features::dpi::DpiSupport;
use serde::{Deserialize, Serialize};

pub const MAX_DPI_LEVELS: usize = 5;
pub const DEFAULT_PROFILE: &str = "Padrão";
const MAX_NAME_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    Invalid(String),
    NotFound(String),
    Exists(String),
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::Invalid(m) => write!(f, "{m}"),
            ProfileError::NotFound(n) => write!(f, "perfil não encontrado: {n}"),
            ProfileError::Exists(n) => write!(f, "já existe um perfil chamado {n}"),
        }
    }
}

impl std::error::Error for ProfileError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub dpi_levels: Vec<u16>,
    pub active_level: usize,
    pub polling_hz: u16,
}

fn check_name(name: &str) -> Result<(), ProfileError> {
    if name.trim().is_empty() {
        return Err(ProfileError::Invalid("o nome do perfil não pode ser vazio".into()));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(ProfileError::Invalid(format!(
            "o nome do perfil excede {MAX_NAME_LEN} caracteres"
        )));
    }
    if name.chars().any(char::is_control) {
        return Err(ProfileError::Invalid(
            "o nome do perfil não pode ter caracteres de controle".into(),
        ));
    }
    Ok(())
}

impl Profile {
    pub fn new(name: &str) -> Self {
        Profile {
            name: name.to_string(),
            dpi_levels: vec![400, 800, 1600, 3200, 6400],
            active_level: 2,
            polling_hz: 1000,
        }
    }

    pub fn active_dpi(&self) -> u16 {
        self.dpi_levels[self.active_level]
    }

    pub fn check_shape(&self) -> Result<(), ProfileError> {
        check_name(&self.name)?;
        if self.dpi_levels.is_empty() || self.dpi_levels.len() > MAX_DPI_LEVELS {
            return Err(ProfileError::Invalid(format!(
                "o perfil precisa de 1 a {MAX_DPI_LEVELS} níveis de DPI"
            )));
        }
        if self.dpi_levels.contains(&0) {
            return Err(ProfileError::Invalid("DPI 0 não é válido".into()));
        }
        if self.active_level >= self.dpi_levels.len() {
            return Err(ProfileError::Invalid(
                "o nível ativo está fora da lista de níveis".into(),
            ));
        }
        if self.polling_hz == 0 {
            return Err(ProfileError::Invalid("a taxa de polling não pode ser 0".into()));
        }
        Ok(())
    }
}

/// Verifica se `dpi` é um valor que o dispositivo anunciou como suportado.
pub fn dpi_allowed(support: &DpiSupport, dpi: u16) -> bool {
    match support {
        DpiSupport::List(values) => values.contains(&dpi),
        DpiSupport::Range { min, max, step } => {
            dpi >= *min && dpi <= *max && (*step == 0 || (dpi - *min).is_multiple_of(*step))
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    active: String,
    #[serde(default)]
    profile: Vec<Profile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileStore {
    active: String,
    profiles: Vec<Profile>,
    needs_seed: bool,
}

impl ProfileStore {
    pub fn default_store() -> Self {
        ProfileStore {
            active: DEFAULT_PROFILE.to_string(),
            profiles: vec![Profile::new(DEFAULT_PROFILE)],
            needs_seed: true,
        }
    }

    pub fn from_toml(text: &str) -> Result<Self, ProfileError> {
        let file: StoreFile = toml::from_str(text)
            .map_err(|e| ProfileError::Invalid(format!("TOML inválido: {e}")))?;
        if file.profile.is_empty() {
            return Err(ProfileError::Invalid("o arquivo não tem perfis".into()));
        }
        let mut seen: Vec<&str> = Vec::new();
        for p in &file.profile {
            p.check_shape()?;
            if seen.contains(&p.name.as_str()) {
                return Err(ProfileError::Invalid(format!("perfil duplicado: {}", p.name)));
            }
            seen.push(&p.name);
        }
        if !file.profile.iter().any(|p| p.name == file.active) {
            return Err(ProfileError::Invalid(format!(
                "o perfil ativo não existe: {}",
                file.active
            )));
        }
        Ok(ProfileStore {
            active: file.active,
            profiles: file.profile,
            needs_seed: false,
        })
    }

    pub fn to_toml(&self) -> String {
        let file = StoreFile {
            active: self.active.clone(),
            profile: self.profiles.clone(),
        };
        toml::to_string_pretty(&file).expect("os perfis são serializáveis")
    }

    /// Lê o arquivo. Se faltar, devolve o padrão. Se for inválido, guarda uma cópia
    /// `.bak`, devolve o padrão e um aviso.
    pub fn load_or_default(path: &Path) -> (Self, Option<String>) {
        match fs::read_to_string(path) {
            Ok(text) => match Self::from_toml(&text) {
                Ok(store) => (store, None),
                Err(e) => {
                    let bak = path.with_extension("toml.bak");
                    let _ = fs::rename(path, &bak);
                    (
                        Self::default_store(),
                        Some(format!(
                            "{} inválido ({e}); cópia em {}",
                            path.display(),
                            bak.display()
                        )),
                    )
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::default_store(), None),
            Err(e) => (
                Self::default_store(),
                Some(format!("não foi possível ler {}: {e}", path.display())),
            ),
        }
    }

    /// Escrita atômica: arquivo temporário, `fsync` e `rename`.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        let mut file = fs::File::create(&tmp)?;
        file.write_all(self.to_toml().as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    }

    pub fn active(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|p| p.name == self.active)
            .expect("o perfil ativo sempre existe")
    }

    pub fn active_name(&self) -> &str {
        &self.active
    }

    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    pub fn names(&self) -> Vec<String> {
        self.profiles.iter().map(|p| p.name.clone()).collect()
    }

    pub fn activate(&mut self, name: &str) -> Result<(), ProfileError> {
        if self.get(name).is_none() {
            return Err(ProfileError::NotFound(name.to_string()));
        }
        self.active = name.to_string();
        Ok(())
    }

    /// Cria um perfil copiando o ativo. Não o ativa.
    pub fn create(&mut self, name: &str) -> Result<(), ProfileError> {
        check_name(name)?;
        if self.get(name).is_some() {
            return Err(ProfileError::Exists(name.to_string()));
        }
        let mut copy = self.active().clone();
        copy.name = name.to_string();
        self.profiles.push(copy);
        Ok(())
    }

    pub fn delete(&mut self, name: &str) -> Result<(), ProfileError> {
        if self.get(name).is_none() {
            return Err(ProfileError::NotFound(name.to_string()));
        }
        if name == self.active {
            return Err(ProfileError::Invalid(
                "não é possível apagar o perfil ativo".into(),
            ));
        }
        if self.profiles.len() == 1 {
            return Err(ProfileError::Invalid(
                "não é possível apagar o último perfil".into(),
            ));
        }
        self.profiles.retain(|p| p.name != name);
        Ok(())
    }

    /// Substitui o conteúdo do perfil ativo, preservando o nome.
    pub fn update_active(&mut self, mut profile: Profile) {
        profile.name = self.active.clone();
        if let Some(slot) = self.profiles.iter_mut().find(|p| p.name == self.active) {
            *slot = profile;
        }
    }

    pub fn needs_seed(&self) -> bool {
        self.needs_seed
    }

    /// Semeia o perfil ativo com os valores atuais do dispositivo (primeira execução).
    pub fn seed(&mut self, dpi: u16, polling_hz: u16) {
        let mut seeded = self.active().clone();
        seeded.dpi_levels = vec![dpi];
        seeded.active_level = 0;
        seeded.polling_hz = polling_hz;
        self.update_active(seeded);
        self.needs_seed = false;
    }
}

/// `$XDG_CONFIG_HOME/ghub-linux/profiles.toml`, ou `$HOME/.config/ghub-linux/profiles.toml`.
pub fn store_path_from(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg.filter(|v| !v.is_empty()) {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(home.filter(|v| !v.is_empty())?).join(".config"),
    };
    Some(base.join("ghub-linux").join("profiles.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, levels: &[u16], active: usize, hz: u16) -> Profile {
        Profile {
            name: name.into(),
            dpi_levels: levels.to_vec(),
            active_level: active,
            polling_hz: hz,
        }
    }

    #[test]
    fn default_store_is_valid_and_needs_seed() {
        let s = ProfileStore::default_store();
        assert!(s.needs_seed());
        assert_eq!(s.active_name(), DEFAULT_PROFILE);
        assert_eq!(s.active().check_shape(), Ok(()));
        assert_eq!(s.active().active_dpi(), 1600);
    }

    #[test]
    fn seed_replaces_levels_with_device_values_and_clears_flag() {
        let mut s = ProfileStore::default_store();
        s.seed(1200, 500);
        assert!(!s.needs_seed());
        assert_eq!(s.active().dpi_levels, vec![1200]);
        assert_eq!(s.active().active_level, 0);
        assert_eq!(s.active().polling_hz, 500);
    }

    #[test]
    fn toml_roundtrip() {
        let mut s = ProfileStore::default_store();
        s.seed(1200, 1000);
        s.create("Trabalho").unwrap();
        let text = s.to_toml();
        let back = ProfileStore::from_toml(&text).unwrap();
        assert_eq!(back.names(), vec![DEFAULT_PROFILE.to_string(), "Trabalho".to_string()]);
        assert_eq!(back.active().dpi_levels, vec![1200]);
        assert!(!back.needs_seed());
    }

    #[test]
    fn from_toml_rejects_bad_files() {
        assert!(ProfileStore::from_toml("not toml {{{").is_err());
        assert!(ProfileStore::from_toml("active = \"x\"\n").is_err()); // sem perfis
        let dup = "active = \"a\"\n[[profile]]\nname=\"a\"\ndpi_levels=[400]\nactive_level=0\npolling_hz=1000\n[[profile]]\nname=\"a\"\ndpi_levels=[400]\nactive_level=0\npolling_hz=1000\n";
        assert!(ProfileStore::from_toml(dup).is_err());
        let missing_active = "active = \"b\"\n[[profile]]\nname=\"a\"\ndpi_levels=[400]\nactive_level=0\npolling_hz=1000\n";
        assert!(ProfileStore::from_toml(missing_active).is_err());
        let bad_level = "active = \"a\"\n[[profile]]\nname=\"a\"\ndpi_levels=[400]\nactive_level=3\npolling_hz=1000\n";
        assert!(ProfileStore::from_toml(bad_level).is_err());
    }

    #[test]
    fn check_shape_rejects_bad_profiles() {
        assert!(profile("ok", &[400], 0, 1000).check_shape().is_ok());
        assert!(profile("", &[400], 0, 1000).check_shape().is_err());
        assert!(profile("   ", &[400], 0, 1000).check_shape().is_err());
        assert!(profile(&"x".repeat(65), &[400], 0, 1000).check_shape().is_err());
        assert!(profile("a\nb", &[400], 0, 1000).check_shape().is_err());
        assert!(profile("p", &[], 0, 1000).check_shape().is_err());
        assert!(profile("p", &[1, 2, 3, 4, 5, 6], 0, 1000).check_shape().is_err());
        assert!(profile("p", &[400], 1, 1000).check_shape().is_err());
        assert!(profile("p", &[0], 0, 1000).check_shape().is_err());
        assert!(profile("p", &[400], 0, 0).check_shape().is_err());
    }

    #[test]
    fn create_copies_active_and_rejects_duplicates_and_bad_names() {
        let mut s = ProfileStore::default_store();
        s.seed(800, 1000);
        s.create("Trabalho").unwrap();
        assert_eq!(s.get("Trabalho").unwrap().dpi_levels, vec![800]);
        assert!(matches!(s.create("Trabalho"), Err(ProfileError::Exists(_))));
        assert!(matches!(s.create("  "), Err(ProfileError::Invalid(_))));
        assert_eq!(s.active_name(), DEFAULT_PROFILE); // criar não ativa
    }

    #[test]
    fn activate_and_delete_rules() {
        let mut s = ProfileStore::default_store();
        s.create("B").unwrap();
        assert!(matches!(s.activate("zzz"), Err(ProfileError::NotFound(_))));
        s.activate("B").unwrap();
        assert_eq!(s.active_name(), "B");
        assert!(matches!(s.delete("B"), Err(ProfileError::Invalid(_)))); // ativo
        s.delete(DEFAULT_PROFILE).unwrap();
        assert!(matches!(s.delete(DEFAULT_PROFILE), Err(ProfileError::NotFound(_))));
        assert!(matches!(s.delete("B"), Err(ProfileError::Invalid(_)))); // último
    }

    #[test]
    fn update_active_keeps_the_name() {
        let mut s = ProfileStore::default_store();
        s.update_active(profile("outro nome", &[800, 1600], 1, 500));
        assert_eq!(s.active_name(), DEFAULT_PROFILE);
        assert_eq!(s.active().dpi_levels, vec![800, 1600]);
        assert_eq!(s.active().polling_hz, 500);
    }

    #[test]
    fn dpi_allowed_checks_list_and_range() {
        let list = DpiSupport::List(vec![400, 800]);
        assert!(dpi_allowed(&list, 800));
        assert!(!dpi_allowed(&list, 900));
        let range = DpiSupport::Range { min: 100, max: 25600, step: 50 };
        assert!(dpi_allowed(&range, 100));
        assert!(dpi_allowed(&range, 1650));
        assert!(dpi_allowed(&range, 25600));
        assert!(!dpi_allowed(&range, 125));
        assert!(!dpi_allowed(&range, 50));
        assert!(!dpi_allowed(&range, 25650));
    }

    #[test]
    fn missing_file_gives_default_without_warning() {
        let dir = tempfile::tempdir().unwrap();
        let (s, warning) = ProfileStore::load_or_default(&dir.path().join("profiles.toml"));
        assert!(s.needs_seed());
        assert_eq!(warning, None);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_default_is_used() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        fs::write(&path, "isto não é toml {{{").unwrap();
        let (s, warning) = ProfileStore::load_or_default(&path);
        assert!(s.needs_seed());
        assert!(warning.unwrap().contains("profiles.toml.bak"));
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("profiles.toml.bak")).unwrap(),
            "isto não é toml {{{"
        );
    }

    #[test]
    fn empty_file_is_treated_as_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("profiles.toml");
        fs::write(&path, "").unwrap();
        let (_, warning) = ProfileStore::load_or_default(&path);
        assert!(warning.is_some());
    }

    #[test]
    fn save_is_atomic_and_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/profiles.toml");
        let mut s = ProfileStore::default_store();
        s.seed(1200, 1000);
        s.save(&path).unwrap();
        assert!(!dir.path().join("sub/profiles.toml.tmp").exists());
        let (back, warning) = ProfileStore::load_or_default(&path);
        assert_eq!(warning, None);
        assert_eq!(back.active().dpi_levels, vec![1200]);
    }

    #[test]
    fn store_path_resolution() {
        assert_eq!(
            store_path_from(Some("/x"), Some("/home/u")),
            Some(PathBuf::from("/x/ghub-linux/profiles.toml"))
        );
        assert_eq!(
            store_path_from(None, Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/ghub-linux/profiles.toml"))
        );
        assert_eq!(
            store_path_from(Some(""), Some("/home/u")),
            Some(PathBuf::from("/home/u/.config/ghub-linux/profiles.toml"))
        );
        assert_eq!(store_path_from(None, None), None);
    }
}
