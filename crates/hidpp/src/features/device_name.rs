use crate::device::Device;
use crate::error::{HidppError, Result};
use crate::transport::Transport;

pub const DEVICE_NAME: u16 = 0x0005;

/// Lê o nome do dispositivo (função 0: tamanho; função 1: bytes a partir de um deslocamento).
pub fn read<T: Transport>(dev: &mut Device<T>) -> Result<String> {
    let len = usize::from(dev.call(DEVICE_NAME, 0, &[])?[0]);
    let mut out: Vec<u8> = Vec::with_capacity(len);
    while out.len() < len {
        let p = dev.call(DEVICE_NAME, 1, &[out.len() as u8])?;
        let take = (len - out.len()).min(p.len());
        let chunk = &p[..take];
        let nul = chunk.iter().position(|b| *b == 0).unwrap_or(chunk.len());
        if nul == 0 {
            break;
        }
        out.extend_from_slice(&chunk[..nul]);
        if nul < chunk.len() {
            break;
        }
    }
    String::from_utf8(out)
        .map_err(|_| HidppError::UnexpectedResponse("device name is not UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{fb, long_reply, FakeTransport};
    use std::collections::HashMap;

    fn dev(fake: FakeTransport) -> Device<FakeTransport> {
        Device::with_features(fake, 1, HashMap::from([(DEVICE_NAME, 3u8)]))
    }

    #[test]
    fn reads_short_name() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 3, fb(0), &[7]));
        fake.push_incoming(long_reply(1, 3, fb(1), b"G703 LS"));
        assert_eq!(read(&mut dev(fake)), Ok("G703 LS".to_string()));
    }

    #[test]
    fn reads_name_longer_than_one_report() {
        let name = "ABCDEFGHIJKLMNOPQRST";
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 3, fb(0), &[20]));
        fake.push_incoming(long_reply(1, 3, fb(1), &name.as_bytes()[..16]));
        fake.push_incoming(long_reply(1, 3, fb(1), &name.as_bytes()[16..]));
        assert_eq!(read(&mut dev(fake)), Ok(name.to_string()));
    }

    #[test]
    fn empty_chunks_do_not_loop_forever() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 3, fb(0), &[5]));
        fake.push_incoming(long_reply(1, 3, fb(1), &[]));
        assert_eq!(read(&mut dev(fake)), Ok(String::new()));
    }

    #[test]
    fn rejects_non_utf8() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 3, fb(0), &[2]));
        fake.push_incoming(long_reply(1, 3, fb(1), &[0xFF, 0xFE]));
        assert_eq!(
            read(&mut dev(fake)),
            Err(HidppError::UnexpectedResponse("device name is not UTF-8"))
        );
    }

    #[test]
    fn missing_feature_is_reported() {
        let mut d = Device::with_features(FakeTransport::new(), 1, HashMap::new());
        assert_eq!(read(&mut d), Err(HidppError::FeatureNotSupported(DEVICE_NAME)));
    }
}
