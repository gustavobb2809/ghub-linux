use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::Duration;

use crate::error::Result;
use crate::transport::Transport;

/// Transporte sobre `/dev/hidrawN`. Os relatórios incluem o Report ID no primeiro byte.
pub struct HidrawTransport {
    file: File,
}

impl HidrawTransport {
    pub fn open(path: &Path) -> Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(HidrawTransport { file })
    }
}

impl Transport for HidrawTransport {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.file.write_all(data)?;
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let mut pfd = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = timeout.as_millis().min(i32::MAX as u128) as libc::c_int;
        // SAFETY: `pfd` é uma estrutura válida e vive durante toda a chamada; passamos 1 elemento.
        let rc = unsafe { libc::poll(&mut pfd, 1, ms) };
        if rc < 0 {
            let err = io::Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                return Ok(None);
            }
            return Err(err.into());
        }
        if rc == 0 {
            return Ok(None);
        }
        let mut buf = [0u8; 64];
        let n = self.file.read(&mut buf)?;
        Ok(Some(buf[..n].to_vec()))
    }
}
