use crate::error::{HidppError, Result};

pub const REPORT_SHORT: u8 = 0x10;
pub const REPORT_LONG: u8 = 0x11;
pub const SHORT_LEN: usize = 7;
pub const LONG_LEN: usize = 20;
/// ID de software enviado nas requisições (nibble baixo do byte de função; nunca 0).
pub const SW_ID: u8 = 0x01;

const HIDPP2_ERROR_INDEX: u8 = 0xFF;
const HIDPP1_ERROR_INDEX: u8 = 0x8F;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub device_index: u8,
    pub feature_index: u8,
    /// Função no nibble alto, ID de software no baixo.
    pub function_byte: u8,
    pub params: Vec<u8>,
}

impl Report {
    pub fn request(device_index: u8, feature_index: u8, function: u8, params: &[u8]) -> Self {
        Report {
            device_index,
            feature_index,
            function_byte: (function << 4) | SW_ID,
            params: params.to_vec(),
        }
    }

    pub fn function(&self) -> u8 {
        self.function_byte >> 4
    }

    pub fn sw_id(&self) -> u8 {
        self.function_byte & 0x0F
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let (id, len) = if self.params.len() <= SHORT_LEN - 4 {
            (REPORT_SHORT, SHORT_LEN)
        } else if self.params.len() <= LONG_LEN - 4 {
            (REPORT_LONG, LONG_LEN)
        } else {
            return Err(HidppError::Malformed("too many parameters"));
        };
        let mut out = vec![0u8; len];
        out[0] = id;
        out[1] = self.device_index;
        out[2] = self.feature_index;
        out[3] = self.function_byte;
        out[4..4 + self.params.len()].copy_from_slice(&self.params);
        Ok(out)
    }

    pub fn decode(raw: &[u8]) -> Result<Self> {
        let expected = match raw.first() {
            Some(&REPORT_SHORT) => SHORT_LEN,
            Some(&REPORT_LONG) => LONG_LEN,
            Some(_) => return Err(HidppError::Malformed("unknown report id")),
            None => return Err(HidppError::Malformed("empty report")),
        };
        if raw.len() < expected {
            return Err(HidppError::Malformed("truncated report"));
        }
        Ok(Report {
            device_index: raw[1],
            feature_index: raw[2],
            function_byte: raw[3],
            params: raw[4..expected].to_vec(),
        })
    }

    /// Reconhece relatórios de erro.
    ///
    /// HID++ 2.0: `[id, dev, 0xFF, feature_index_que_falhou, função_que_falhou, código]`,
    /// então `function_byte` guarda o índice da feature e `params[1]` o código.
    /// HID++ 1.0: `[id, dev, 0x8F, sub_id, endereço, código, 0]`.
    pub fn as_error(&self) -> Option<HidppError> {
        let code = self.params.get(1).copied().unwrap_or(0);
        match self.feature_index {
            HIDPP2_ERROR_INDEX => Some(HidppError::Protocol {
                feature_index: self.function_byte,
                code,
            }),
            HIDPP1_ERROR_INDEX => Some(HidppError::LegacyProtocol { code }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padded(mut v: Vec<u8>, len: usize) -> Vec<u8> {
        v.resize(len, 0);
        v
    }

    #[test]
    fn encodes_short_request() {
        let r = Report::request(0x01, 0x00, 1, &[0x00, 0x00, 0xA5]);
        assert_eq!(
            r.encode().unwrap(),
            vec![0x10, 0x01, 0x00, 0x11, 0x00, 0x00, 0xA5]
        );
    }

    #[test]
    fn encodes_long_when_params_exceed_three() {
        let r = Report::request(0x01, 0x02, 3, &[1, 2, 3, 4]);
        let out = r.encode().unwrap();
        assert_eq!(out.len(), 20);
        assert_eq!(out[0], 0x11);
        assert_eq!(&out[..8], &[0x11, 0x01, 0x02, 0x31, 1, 2, 3, 4]);
    }

    #[test]
    fn encode_rejects_more_than_sixteen_params() {
        let r = Report::request(0x01, 0x02, 0, &[0u8; 17]);
        assert_eq!(
            r.encode(),
            Err(HidppError::Malformed("too many parameters"))
        );
    }

    #[test]
    fn decode_long_roundtrip() {
        let raw = padded(vec![0x11, 0x02, 0x05, 0x21, 9, 8, 7], 20);
        let r = Report::decode(&raw).unwrap();
        assert_eq!(r.device_index, 0x02);
        assert_eq!(r.feature_index, 0x05);
        assert_eq!(r.function(), 2);
        assert_eq!(r.sw_id(), 1);
        assert_eq!(&r.params[..3], &[9, 8, 7]);
        assert_eq!(r.params.len(), 16);
    }

    #[test]
    fn decode_rejects_truncated_unknown_and_empty() {
        assert_eq!(
            Report::decode(&[0x11, 1, 0, 0x11]),
            Err(HidppError::Malformed("truncated report"))
        );
        assert_eq!(
            Report::decode(&[0x20, 0, 0, 0, 0, 0, 0]),
            Err(HidppError::Malformed("unknown report id"))
        );
        assert_eq!(
            Report::decode(&[]),
            Err(HidppError::Malformed("empty report"))
        );
    }

    #[test]
    fn classifies_hidpp2_error() {
        let raw = padded(vec![0x11, 0x01, 0xFF, 0x04, 0x21, 0x05], 20);
        let r = Report::decode(&raw).unwrap();
        assert_eq!(
            r.as_error(),
            Some(HidppError::Protocol {
                feature_index: 4,
                code: 5
            })
        );
    }

    #[test]
    fn classifies_legacy_error() {
        let r = Report::decode(&[0x10, 0x01, 0x8F, 0x00, 0x11, 0x01, 0x00]).unwrap();
        assert_eq!(r.as_error(), Some(HidppError::LegacyProtocol { code: 1 }));
    }

    #[test]
    fn normal_report_is_not_an_error() {
        let r = Report::decode(&[0x10, 0x01, 0x04, 0x11, 0, 0, 0]).unwrap();
        assert_eq!(r.as_error(), None);
    }
}
