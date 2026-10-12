//! Cliente HID++ 2.0 para dispositivos Logitech.

pub mod device;
pub mod error;
pub mod report;
pub mod transport;

pub use error::{HidppError, Result};
