use ghub_ipc::DeviceState;

pub const MAX_LEVELS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DpiRange { pub min: u32, pub max: u32, pub step: u32, pub list: Vec<u32> }

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Levels { pub levels: Vec<u32>, pub active: u32 }

impl DpiRange {
    pub fn from_state(s: &DeviceState) -> Self {
        DpiRange { min: s.dpi_min, max: s.dpi_max, step: s.dpi_step, list: s.dpi_list.clone() }
    }

    fn max_msg(&self) -> String {
        if self.max == 0 { "Faixa de DPI desconhecida.".into() } else { format!("O DPI máximo é {}.", self.max) }
    }

    pub fn validate(&self, dpi: i64) -> Result<u32, String> {
        if self.max == 0 && self.list.is_empty() {
            return Err("Faixa de DPI desconhecida (mouse desconectado).".into());
        }
        if dpi <= 0 {
            return Err("Informe um DPI maior que zero.".into());
        }
        let v = u32::try_from(dpi).map_err(|_| self.max_msg())?;
        if !self.list.is_empty() {
            return if self.list.contains(&v) { Ok(v) } else { Err("Este DPI não é suportado pelo mouse.".into()) };
        }
        if v < self.min { return Err(format!("O DPI mínimo é {}.", self.min)); }
        if v > self.max { return Err(self.max_msg()); }
        if self.step > 0 && (v - self.min) % self.step != 0 {
            return Err(format!("Use múltiplos de {} (a partir de {}).", self.step, self.min));
        }
        Ok(v)
    }

    pub fn snap(&self, dpi: u32) -> u32 {
        if let Some(c) = self.list.iter().min_by_key(|c| c.abs_diff(dpi)) { return *c; }
        let v = dpi.clamp(self.min, self.max.max(self.min));
        if self.step == 0 { return v; }
        let r = (v - self.min) % self.step;
        let down = v - r;
        if r.saturating_mul(2) >= self.step && down + self.step <= self.max { down + self.step } else { down }
    }
}

pub fn set_level(l: &Levels, r: &DpiRange, index: usize, dpi: i64) -> Result<Levels, String> {
    if index >= l.levels.len() { return Err("Nível inexistente.".into()); }
    let v = r.validate(dpi)?;
    let mut out = l.clone();
    out.levels[index] = v;
    Ok(out)
}

pub fn add_level(l: &Levels, r: &DpiRange) -> Result<Levels, String> {
    if l.levels.len() >= MAX_LEVELS {
        return Err(format!("O máximo é {MAX_LEVELS} níveis."));
    }
    let last = l.levels.last().copied().unwrap_or(r.min);
    let mut out = l.clone();
    let v = r.snap(last.saturating_mul(2));
    r.validate(i64::from(v))?;
    out.levels.push(v);
    Ok(out)
}

pub fn remove_level(l: &Levels, index: usize) -> Result<Levels, String> {
    if l.levels.len() <= 1 { return Err("É preciso manter ao menos um nível.".into()); }
    if index >= l.levels.len() { return Err("Nível inexistente.".into()); }
    let mut out = l.clone();
    out.levels.remove(index);
    let a = l.active as usize;
    let new_active = if index < a { a - 1 } else { a };
    out.active = new_active.min(out.levels.len() - 1) as u32;
    Ok(out)
}

pub fn select_level(l: &Levels, index: usize) -> Result<Levels, String> {
    if index >= l.levels.len() { return Err("Nível inexistente.".into()); }
    let mut out = l.clone();
    out.active = index as u32;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn range() -> DpiRange { DpiRange { min: 100, max: 25600, step: 50, list: vec![] } }

    #[test]
    fn validate_accepts_values_on_the_step_grid() {
        assert_eq!(range().validate(1200), Ok(1200));
        assert_eq!(range().validate(100), Ok(100));
        assert_eq!(range().validate(25600), Ok(25600));
    }
    #[test]
    fn validate_rejects_bad_input() {
        let r = range();
        for bad in [0_i64, -5, 99, 125, 25650, 26000, i64::MAX, i64::MIN] {
            assert!(r.validate(bad).is_err(), "{bad} deveria falhar");
        }
    }
    #[test]
    fn validate_with_explicit_list_ignores_min_max_step() {
        let r = DpiRange { min: 100, max: 3200, step: 0, list: vec![400, 800, 1600] };
        assert_eq!(r.validate(800), Ok(800));
        assert!(r.validate(801).is_err());
    }
    #[test]
    fn unknown_range_rejects_everything() {
        let r = DpiRange::default();
        assert!(r.validate(800).unwrap_err().contains("desconhecida"));
    }
    #[test]
    fn snap_rounds_to_step_and_clamps() {
        let r = range();
        assert_eq!(r.snap(1224), 1200);
        assert_eq!(r.snap(1226), 1250);
        assert_eq!(r.snap(10), 100);
        assert_eq!(r.snap(99999), 25600);
        let l = DpiRange { min: 100, max: 3200, step: 0, list: vec![400, 800, 1600] };
        assert_eq!(l.snap(1000), 800);
    }
    #[test]
    fn add_level_never_adds_invalid_values() {
        let l = Levels { levels: vec![400], active: 0 };
        assert!(add_level(&l, &DpiRange::default()).is_err());
        let inv = DpiRange { min: 800, max: 100, step: 50, list: vec![] };
        assert!(add_level(&l, &inv).is_err());
        assert_eq!(l.levels, vec![400]);
    }
    #[test]
    fn level_edits_respect_limits_and_active_index() {
        let r = range();
        let l = Levels { levels: vec![400, 800, 1600], active: 1 };
        assert_eq!(set_level(&l, &r, 0, 500).unwrap().levels, vec![500, 800, 1600]);
        assert!(set_level(&l, &r, 0, 501).is_err());
        assert!(set_level(&l, &r, 9, 500).is_err());
        let added = add_level(&l, &r).unwrap();
        assert_eq!(added.levels.len(), 4);
        let full = Levels { levels: vec![400; MAX_LEVELS], active: 0 };
        assert!(add_level(&full, &r).is_err());
        let removed = remove_level(&l, 0).unwrap();
        assert_eq!((removed.levels.clone(), removed.active), (vec![800, 1600], 0));
        let removed_active = remove_level(&l, 1).unwrap();
        assert_eq!((removed_active.levels, removed_active.active), (vec![400, 1600], 1));
        assert!(remove_level(&Levels { levels: vec![800], active: 0 }, 0).is_err());
        assert_eq!(select_level(&l, 2).unwrap().active, 2);
        assert!(select_level(&l, 3).is_err());
    }
}
