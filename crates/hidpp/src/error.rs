use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq, Clone)]
pub enum HidppError {
    #[error("relatório HID++ malformado: {0}")]
    Malformed(&'static str),
    #[error("tempo esgotado esperando o dispositivo")]
    Timeout,
    #[error("erro HID++ 2.0 0x{code:02x} na feature de índice 0x{feature_index:02x}")]
    Protocol { feature_index: u8, code: u8 },
    #[error("erro HID++ 1.0 0x{code:02x} (dispositivo não fala HID++ 2.0)")]
    LegacyProtocol { code: u8 },
    #[error("o dispositivo não suporta a feature 0x{0:04x}")]
    FeatureNotSupported(u16),
    #[error("resposta inesperada: {0}")]
    UnexpectedResponse(&'static str),
    #[error("argumento inválido: {0}")]
    InvalidArgument(&'static str),
    #[error("erro de E/S: {0}")]
    Io(String),
}

impl From<std::io::Error> for HidppError {
    fn from(e: std::io::Error) -> Self {
        HidppError::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, HidppError>;
