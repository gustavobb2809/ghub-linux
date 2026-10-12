use std::collections::VecDeque;
use std::time::Duration;

use crate::error::Result;

pub trait Transport {
    fn write(&mut self, data: &[u8]) -> Result<()>;
    /// `Ok(None)` quando o tempo esgota sem nada para ler.
    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>>;
}

/// Transporte roteirizado para testes: devolve os relatórios enfileirados, na ordem,
/// e `None` (tempo esgotado) quando a fila acaba.
#[derive(Default)]
pub struct FakeTransport {
    pub written: Vec<Vec<u8>>,
    incoming: VecDeque<Vec<u8>>,
}

impl FakeTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_incoming(&mut self, data: Vec<u8>) {
        self.incoming.push_back(data);
    }
}

impl Transport for FakeTransport {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.written.push(data.to_vec());
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>> {
        Ok(self.incoming.pop_front())
    }
}

/// Monta um relatório longo (20 bytes) como um dispositivo o enviaria. Útil em testes.
pub fn long_reply(
    device_index: u8,
    feature_index: u8,
    function_byte: u8,
    params: &[u8],
) -> Vec<u8> {
    let mut v = vec![0x11, device_index, feature_index, function_byte];
    v.extend_from_slice(params);
    v.resize(20, 0);
    v
}
