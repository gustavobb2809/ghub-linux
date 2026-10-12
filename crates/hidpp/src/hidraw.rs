use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::error::{HidppError, Result};
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

    /// Envolve um arquivo já aberto (usado nos testes com pipes).
    pub fn from_file(file: File) -> Self {
        HidrawTransport { file }
    }
}

impl Transport for HidrawTransport {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.file.write_all(data)?;
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let mut pfd = libc::pollfd {
                fd: self.file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            let ms = remaining.as_millis().min(i32::MAX as u128) as libc::c_int;
            // SAFETY: `pfd` é uma estrutura válida e vive durante toda a chamada; passamos 1 elemento.
            let rc = unsafe { libc::poll(&mut pfd, 1, ms) };
            if rc < 0 {
                let err = io::Error::last_os_error();
                if err.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(err.into());
            }
            if rc == 0 {
                return Ok(None);
            }
            if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(HidppError::Io(
                    "hidraw device error or disconnected".to_string(),
                ));
            }
            let mut buf = [0u8; 64];
            let n = self.file.read(&mut buf)?;
            if n == 0 {
                return Err(HidppError::Io("hidraw returned an empty read".to_string()));
            }
            return Ok(Some(buf[..n].to_vec()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::pipe;
    use std::os::fd::OwnedFd;

    fn pipe_transport() -> (HidrawTransport, std::io::PipeWriter) {
        let (reader, writer) = pipe().unwrap();
        (
            HidrawTransport::from_file(File::from(OwnedFd::from(reader))),
            writer,
        )
    }

    #[test]
    fn read_times_out_when_nothing_arrives() {
        let (mut t, _writer) = pipe_transport();
        let start = Instant::now();
        assert_eq!(t.read(Duration::from_millis(50)), Ok(None));
        assert!(start.elapsed() >= Duration::from_millis(40));
    }

    #[test]
    fn read_returns_available_bytes() {
        let (mut t, mut writer) = pipe_transport();
        writer.write_all(&[0x11, 1, 2, 3]).unwrap();
        assert_eq!(
            t.read(Duration::from_millis(200)),
            Ok(Some(vec![0x11, 1, 2, 3]))
        );
    }

    #[test]
    fn read_reports_hangup_as_io_error() {
        let (mut t, writer) = pipe_transport();
        drop(writer);
        assert!(matches!(
            t.read(Duration::from_millis(200)),
            Err(HidppError::Io(_))
        ));
    }
}
