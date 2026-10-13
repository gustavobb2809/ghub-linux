//! Transporte falso com uma memória onboard simulada (diretório + um perfil), para testes.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use super::onboard::{seal, Led, Sector, LED_LEN, LED_OFFSETS, SECTOR_SIZE};
use crate::error::Result;
use crate::transport::{long_reply, Transport};

/// Setor de perfil válido (CRC selado) com `led` nas quatro entradas.
pub fn sample_profile_sector(led: &Led) -> Sector {
    let mut s: Sector = [0xff; SECTOR_SIZE];
    s[0] = 0x01; // taxa
    for o in LED_OFFSETS {
        s[o..o + LED_LEN].copy_from_slice(&led.encode());
    }
    seal(&mut s);
    s
}

pub struct MemOnboard {
    feature_index: u8,
    pub sectors: HashMap<u16, Sector>,
    profile_number: u8,
    pub reloads: Vec<u8>,
    /// Setores efetivamente gravados (um por `fn8`).
    pub commits: Vec<u16>,
    /// A próxima gravação sai com um byte trocado (simula falha de flash).
    pub corrupt_next_commit: bool,
    cursor: u16,
    staging: Vec<u8>,
    incoming: VecDeque<Vec<u8>>,
}

impl MemOnboard {
    /// Perfil 1 no setor 1, diretório no setor 0, com `led` nas quatro entradas.
    pub fn new(feature_index: u8, led: &Led) -> Self {
        let mut dir: Sector = [0xff; SECTOR_SIZE];
        dir[..4].copy_from_slice(&[0x00, 0x01, 0x01, 0x00]);
        seal(&mut dir);
        MemOnboard {
            feature_index,
            sectors: HashMap::from([(0, dir), (1, sample_profile_sector(led))]),
            profile_number: 1,
            reloads: vec![],
            commits: vec![],
            corrupt_next_commit: false,
            cursor: 0,
            staging: vec![],
            incoming: VecDeque::new(),
        }
    }
}

impl Transport for MemOnboard {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        assert_eq!(data[2], self.feature_index, "feature errada");
        let (func, p) = (data[3] >> 4, &data[4..]);
        let reply: Vec<u8> = match func {
            3 => {
                self.reloads.push(p[1]);
                vec![]
            }
            4 => vec![0, self.profile_number],
            5 => {
                let sector = u16::from_be_bytes([p[0], p[1]]);
                let off = usize::from(u16::from_be_bytes([p[2], p[3]]));
                let mem = &self.sectors[&sector];
                (0..16).map(|i| *mem.get(off + i).unwrap_or(&0xff)).collect()
            }
            6 => {
                self.cursor = u16::from_be_bytes([p[0], p[1]]);
                self.staging.clear();
                vec![]
            }
            7 => {
                self.staging.extend_from_slice(&p[..16]);
                vec![]
            }
            8 => {
                let mut s: Sector = [0; SECTOR_SIZE];
                s.copy_from_slice(&self.staging[..SECTOR_SIZE]);
                if std::mem::take(&mut self.corrupt_next_commit) {
                    s[10] ^= 0xff;
                }
                self.sectors.insert(self.cursor, s);
                self.commits.push(self.cursor);
                vec![]
            }
            other => panic!("função {other} não simulada"),
        };
        self.incoming.push_back(long_reply(data[1], data[2], data[3], &reply));
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>> {
        Ok(self.incoming.pop_front())
    }
}
