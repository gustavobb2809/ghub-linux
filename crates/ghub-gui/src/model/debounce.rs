use std::time::{Duration, Instant};

/// Mantém só o último valor empurrado; ele vence após `delay` sem novos valores.
#[derive(Debug)]
pub struct Debouncer<T> { delay: Duration, pending: Option<(T, Instant)> }

impl<T> Debouncer<T> {
    pub fn new(delay: Duration) -> Self { Debouncer { delay, pending: None } }
    /// Substitui o valor pendente e reinicia o prazo.
    pub fn push(&mut self, value: T, now: Instant) { self.pending = Some((value, now + self.delay)); }
    pub fn take_due(&mut self, now: Instant) -> Option<T> {
        match &self.pending {
            Some((_, due)) if *due <= now => self.pending.take().map(|(v, _)| v),
            _ => None,
        }
    }
    pub fn flush(&mut self) -> Option<T> { self.pending.take().map(|(v, _)| v) }
    pub fn peek(&self) -> Option<&T> { self.pending.as_ref().map(|(v, _)| v) }
    pub fn next_deadline(&self) -> Option<Instant> { self.pending.as_ref().map(|(_, d)| *d) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_last_value_survives_a_burst() {
        let t0 = Instant::now();
        let mut d = Debouncer::new(Duration::from_millis(150));
        for (i, ms) in [0u64, 20, 40, 60].iter().enumerate() {
            d.push(i, t0 + Duration::from_millis(*ms));
        }
        assert_eq!(d.take_due(t0 + Duration::from_millis(100)), None); // prazo reiniciado
        assert_eq!(d.peek(), Some(&3));
        assert_eq!(d.take_due(t0 + Duration::from_millis(211)), Some(3));
        assert_eq!(d.take_due(t0 + Duration::from_secs(9)), None);
    }
    #[test]
    fn flush_returns_pending_immediately() {
        let t0 = Instant::now();
        let mut d = Debouncer::new(Duration::from_secs(1));
        d.push("x", t0);
        assert_eq!(d.flush(), Some("x"));
        assert_eq!(d.flush(), None);
        assert_eq!(d.next_deadline(), None);
    }
}
