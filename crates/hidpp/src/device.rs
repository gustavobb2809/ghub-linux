use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::error::{HidppError, Result};
use crate::report::Report;
use crate::transport::Transport;

pub const ROOT_FEATURE_INDEX: u8 = 0x00;
pub const FEATURE_SET: u16 = 0x0001;
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(2);

const PING_BYTE: u8 = 0xA5;

pub struct Device<T: Transport> {
    transport: T,
    device_index: u8,
    timeout: Duration,
    features: HashMap<u16, u8>,
}

impl<T: Transport> Device<T> {
    pub fn new(transport: T, device_index: u8) -> Self {
        Self::with_features(transport, device_index, HashMap::new())
    }

    /// Cria um dispositivo com o mapa de features já conhecido (útil em testes).
    pub fn with_features(
        transport: T,
        device_index: u8,
        features: HashMap<u16, u8>,
    ) -> Self {
        Device {
            transport,
            device_index,
            timeout: DEFAULT_TIMEOUT,
            features,
        }
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn device_index(&self) -> u8 {
        self.device_index
    }

    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Envia uma requisição e devolve os parâmetros da resposta correspondente.
    ///
    /// Relatórios de outros dispositivos, notificações, lixo e erros de requisições
    /// alheias (outra feature, função ou SW_ID) são ignorados.
    pub fn request(&mut self, feature_index: u8, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        let req = Report::request(self.device_index, feature_index, function, params);
        self.transport.write(&req.encode()?)?;
        let deadline = Instant::now() + self.timeout;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(HidppError::Timeout);
            }
            let Some(raw) = self.transport.read(remaining)? else {
                return Err(HidppError::Timeout);
            };
            let Ok(resp) = Report::decode(&raw) else {
                continue;
            };
            if resp.device_index != self.device_index {
                continue;
            }
            if let Some(err) = resp.as_error() {
                // Em ambos os formatos de erro, o byte 2 do relatório guarda o índice da
                // feature (2.0) ou o SubID (1.0) da requisição, e o primeiro parâmetro o
                // byte de função (com SW_ID) ou o endereço. Só é nosso se ambos baterem.
                let ours = resp.function_byte == req.feature_index
                    && resp.params.first() == Some(&req.function_byte);
                if ours {
                    return Err(err);
                }
                continue;
            }
            if resp.feature_index == req.feature_index && resp.function_byte == req.function_byte {
                return Ok(resp.params);
            }
        }
    }

    /// IRoot.GetProtocolVersion. Devolve `(maior, menor)`.
    ///
    /// Um dispositivo HID++ 1.0 responde com `LegacyProtocol`.
    pub fn ping(&mut self) -> Result<(u8, u8)> {
        let p = self.request(ROOT_FEATURE_INDEX, 1, &[0, 0, PING_BYTE])?;
        if p[2] != PING_BYTE {
            return Err(HidppError::UnexpectedResponse("ping byte not echoed"));
        }
        Ok((p[0], p[1]))
    }

    /// Descobre o mapa feature-ID → índice via IRoot e IFeatureSet.
    pub fn discover(&mut self) -> Result<()> {
        let p = self.request(ROOT_FEATURE_INDEX, 0, &FEATURE_SET.to_be_bytes())?;
        let set_index = p[0];
        if set_index == 0 {
            return Err(HidppError::FeatureNotSupported(FEATURE_SET));
        }
        let count = self.request(set_index, 0, &[])?[0];
        let mut map = HashMap::new();
        map.insert(FEATURE_SET, set_index);
        for index in 1..=count {
            let p = self.request(set_index, 1, &[index])?;
            let id = u16::from_be_bytes([p[0], p[1]]);
            if id == 0 {
                continue;
            }
            map.insert(id, index);
        }
        self.features = map;
        Ok(())
    }

    pub fn feature_index(&self, id: u16) -> Result<u8> {
        self.features
            .get(&id)
            .copied()
            .ok_or(HidppError::FeatureNotSupported(id))
    }

    /// Features conhecidas como `(id, índice)`, ordenadas por índice.
    pub fn features(&self) -> Vec<(u16, u8)> {
        let mut v: Vec<(u16, u8)> = self.features.iter().map(|(id, idx)| (*id, *idx)).collect();
        v.sort_by_key(|(id, idx)| (*idx, *id));
        v
    }

    /// Chama uma função de uma feature pelo ID dela.
    pub fn call(&mut self, feature_id: u16, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        let index = self.feature_index(feature_id)?;
        self.request(index, function, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{fb, long_reply, FakeTransport};

    fn dev(fake: FakeTransport) -> Device<FakeTransport> {
        Device::new(fake, 1)
    }

    #[test]
    fn request_returns_matching_response() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0x00, fb(1), &[4, 2, 0xA5]));
        let mut d = dev(fake);
        let p = d.request(0x00, 1, &[0, 0, 0xA5]).unwrap();
        assert_eq!(&p[..3], &[4, 2, 0xA5]);
    }

    #[test]
    fn request_skips_notifications() {
        let mut fake = FakeTransport::new();
        // Notificação de outra feature (bateria) chega antes da resposta.
        fake.push_incoming(long_reply(1, 0x05, 0x00, &[77, 0, 0]));
        fake.push_incoming(long_reply(1, 0x00, fb(1), &[4, 2, 0xA5]));
        let mut d = dev(fake);
        assert!(d.request(0x00, 1, &[0, 0, 0xA5]).is_ok());
    }

    #[test]
    fn request_times_out_without_response() {
        let mut d = dev(FakeTransport::new());
        assert_eq!(d.request(0x00, 1, &[]), Err(HidppError::Timeout));
    }

    #[test]
    fn request_ignores_malformed_and_foreign_reports() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(vec![0x20, 0, 0]); // ID desconhecido
        fake.push_incoming(vec![0x11, 1]); // truncado
        fake.push_incoming(long_reply(2, 0x00, fb(1), &[9, 9, 9])); // outro índice de dispositivo
        fake.push_incoming(long_reply(1, 0x00, fb(1), &[4, 2, 0xA5]));
        let mut d = dev(fake);
        let p = d.request(0x00, 1, &[0, 0, 0xA5]).unwrap();
        assert_eq!(&p[..3], &[4, 2, 0xA5]);
    }

    #[test]
    fn request_maps_hidpp2_error() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0xFF, 0x04, &[fb(2), 0x02]));
        let mut d = dev(fake);
        assert_eq!(
            d.request(0x04, 2, &[]),
            Err(HidppError::Protocol {
                feature_index: 4,
                code: 2
            })
        );
    }

    #[test]
    fn request_skips_reply_with_foreign_sw_id() {
        let mut fake = FakeTransport::new();
        // Mesma feature e função, mas SW_ID 0x01 (o do driver do kernel).
        fake.push_incoming(long_reply(1, 0x00, 0x11, &[9, 9, 9]));
        fake.push_incoming(long_reply(1, 0x00, fb(1), &[4, 2, 0xA5]));
        let mut d = dev(fake);
        let p = d.request(0x00, 1, &[0, 0, 0xA5]).unwrap();
        assert_eq!(&p[..3], &[4, 2, 0xA5]);
    }

    #[test]
    fn request_skips_foreign_hidpp2_errors() {
        let mut fake = FakeTransport::new();
        // Erro de outra feature.
        fake.push_incoming(long_reply(1, 0xFF, 0x05, &[fb(2), 0x02]));
        // Erro da mesma feature, mas de uma requisição do kernel (SW_ID 0x01).
        fake.push_incoming(long_reply(1, 0xFF, 0x04, &[0x21, 0x02]));
        // Erro da mesma feature, outra função nossa.
        fake.push_incoming(long_reply(1, 0xFF, 0x04, &[fb(1), 0x02]));
        fake.push_incoming(long_reply(1, 0x04, fb(2), &[7]));
        let mut d = dev(fake);
        assert_eq!(d.request(0x04, 2, &[]).unwrap()[0], 7);
    }

    #[test]
    fn request_skips_foreign_hidpp1_errors() {
        let mut fake = FakeTransport::new();
        // Erro 1.0 da requisição do kernel (SW_ID 0x01) para o mesmo SubID.
        fake.push_incoming(vec![0x10, 0x01, 0x8F, 0x00, 0x11, 0x01, 0x00]);
        // Erro 1.0 para outro SubID.
        fake.push_incoming(vec![0x10, 0x01, 0x8F, 0x83, 0xB5, 0x02, 0x00]);
        fake.push_incoming(long_reply(1, 0x00, fb(1), &[4, 2, 0xA5]));
        let mut d = dev(fake);
        assert_eq!(d.ping(), Ok((4, 2)));
    }

    #[test]
    fn request_maps_matching_hidpp1_error_on_feature() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(vec![0x10, 0x01, 0x8F, 0x04, fb(2), 0x03, 0x00]);
        let mut d = dev(fake);
        assert_eq!(
            d.request(0x04, 2, &[]),
            Err(HidppError::LegacyProtocol { code: 3 })
        );
    }

    #[test]
    fn ping_returns_protocol_version() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0x00, fb(1), &[4, 2, 0xA5]));
        let mut d = dev(fake);
        assert_eq!(d.ping(), Ok((4, 2)));
    }

    #[test]
    fn ping_reports_legacy_device() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(vec![0x10, 0x01, 0x8F, 0x00, fb(1), 0x01, 0x00]);
        let mut d = dev(fake);
        assert_eq!(d.ping(), Err(HidppError::LegacyProtocol { code: 1 }));
    }

    #[test]
    fn ping_requires_echo() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0x00, fb(1), &[4, 2, 0x00]));
        let mut d = dev(fake);
        assert_eq!(
            d.ping(),
            Err(HidppError::UnexpectedResponse("ping byte not echoed"))
        );
    }

    #[test]
    fn discover_maps_features_and_rejects_unknown() {
        let mut fake = FakeTransport::new();
        // IRoot.GetFeature(IFeatureSet) -> índice 1
        fake.push_incoming(long_reply(1, 0x00, fb(0), &[1, 0, 0]));
        // IFeatureSet.GetCount -> 2 features além da IRoot
        fake.push_incoming(long_reply(1, 0x01, fb(0), &[2]));
        // GetFeatureID(1) -> 0x1004; GetFeatureID(2) -> 0x2201
        fake.push_incoming(long_reply(1, 0x01, fb(1), &[0x10, 0x04, 0]));
        fake.push_incoming(long_reply(1, 0x01, fb(1), &[0x22, 0x01, 0]));
        let mut d = dev(fake);
        d.discover().unwrap();
        assert_eq!(d.feature_index(0x1004), Ok(1));
        assert_eq!(d.feature_index(0x2201), Ok(2));
        assert_eq!(
            d.feature_index(0x8060),
            Err(HidppError::FeatureNotSupported(0x8060))
        );
        assert_eq!(d.features(), vec![(0x0001, 1), (0x1004, 1), (0x2201, 2)]);
    }

    #[test]
    fn discover_ignores_feature_id_zero() {
        let mut fake = FakeTransport::new();
        fake.push_incoming(long_reply(1, 0x00, fb(0), &[1, 0, 0]));
        fake.push_incoming(long_reply(1, 0x01, fb(0), &[2]));
        fake.push_incoming(long_reply(1, 0x01, fb(1), &[0x00, 0x00, 0]));
        fake.push_incoming(long_reply(1, 0x01, fb(1), &[0x22, 0x01, 0]));
        let mut d = dev(fake);
        d.discover().unwrap();
        assert_eq!(d.features(), vec![(0x0001, 1), (0x2201, 2)]);
    }

    #[test]
    fn call_on_unsupported_feature_does_not_touch_the_wire() {
        let mut d = dev(FakeTransport::new());
        assert_eq!(
            d.call(0x2201, 0, &[]),
            Err(HidppError::FeatureNotSupported(0x2201))
        );
        assert!(d.into_transport().written.is_empty());
    }
}
