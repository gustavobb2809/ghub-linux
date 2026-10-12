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

/// Espera até `deadline` por algo legível. `Ok(false)` quando nada chegou a tempo.
fn wait_readable(file: &File, deadline: Instant) -> Result<bool> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let ms = remaining.as_millis().min(i32::MAX as u128) as libc::c_int;
        let mut pfd = libc::pollfd {
            fd: file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
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
            return Ok(false);
        }
        if pfd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            return Err(HidppError::Io(
                "hidraw device error or disconnected".to_string(),
            ));
        }
        return Ok(true);
    }
}

impl HidrawTransport {
    fn read_available(&mut self) -> Result<Vec<u8>> {
        let mut buf = [0u8; 64];
        let n = self.file.read(&mut buf)?;
        if n == 0 {
            return Err(HidppError::Io("hidraw returned an empty read".to_string()));
        }
        Ok(buf[..n].to_vec())
    }
}

impl Transport for HidrawTransport {
    fn write(&mut self, data: &[u8]) -> Result<()> {
        self.file.write_all(data)?;
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        if !wait_readable(&self.file, Instant::now() + timeout)? {
            return Ok(None);
        }
        self.read_available().map(Some)
    }

    /// O hidraw entrega cada relatório de entrada a todos os descritores abertos no nó;
    /// o que outro descritor pediu fica na nossa fila e é descartado aqui.
    fn discard_pending(&mut self) -> Result<()> {
        while wait_readable(&self.file, Instant::now())? {
            self.read_available()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Device;
    use crate::transport::{fb, long_reply};
    use std::io::pipe;
    use std::os::fd::OwnedFd;
    use std::os::unix::net::UnixStream;

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

    #[test]
    fn discard_pending_on_empty_pipe_returns_immediately() {
        let (mut t, _writer) = pipe_transport();
        let start = Instant::now();
        assert_eq!(t.discard_pending(), Ok(()));
        assert!(start.elapsed() < Duration::from_millis(20));
    }

    #[test]
    fn discard_pending_drains_queued_bytes() {
        let (mut t, mut writer) = pipe_transport();
        writer.write_all(&[0x11, 1, 2, 3]).unwrap();
        writer.write_all(&[0x11, 4, 5, 6]).unwrap();
        assert_eq!(t.discard_pending(), Ok(()));
        assert_eq!(t.read(Duration::from_millis(20)), Ok(None));
    }

    #[test]
    fn discard_pending_reports_hangup_as_io_error() {
        let (mut t, writer) = pipe_transport();
        drop(writer);
        assert!(matches!(t.discard_pending(), Err(HidppError::Io(_))));
    }

    #[test]
    fn stale_reply_queued_before_the_request_is_not_taken_as_the_answer() {
        let (ours, mut peer) = UnixStream::pair().unwrap();
        let t = HidrawTransport::from_file(File::from(OwnedFd::from(ours)));
        // Resposta válida de um ping feito por outro descritor, já na fila.
        peer.write_all(&long_reply(1, 0x00, fb(1), &[4, 2, 0xA5])).unwrap();
        let mut dev = Device::new(t, 1);
        dev.set_timeout(Duration::from_millis(50));
        assert_eq!(dev.ping(), Err(HidppError::Timeout));
    }
}
