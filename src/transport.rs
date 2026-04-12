//! [`ScdcTransport`] implementation backed by a `/dev/i2c-N` device node.
//!
//! [`ScdcTransport`]: hdmi_hal::scdc::ScdcTransport

use std::path::Path;
use std::sync::Mutex;

use i2cdev::core::{I2CMessage, I2CTransfer};
use i2cdev::linux::{LinuxI2CBus, LinuxI2CError, LinuxI2CMessage};

use hdmi_hal::scdc::ScdcTransport;

use crate::error::{I2cDevError, I2cErrorKind, I2cTransactionError, MessagePhase};

/// SCDC slave address, fixed by the HDMI specification (HDMI 2.1 Table 10-3).
const SCDC_ADDRESS: u16 = 0x54;

/// An SCDC transport backed by a `/dev/i2c-N` device node.
///
/// Implements [`ScdcTransport`] by issuing compound `I2C_RDWR` transactions
/// to the SCDC slave address (0x54) on the specified adapter.
///
/// # Error recovery
///
/// A [`I2cDevError::Transaction`] error does not invalidate the transport. The
/// underlying file descriptor remains open and subsequent calls may succeed.
/// If errors persist, the sink has most likely disconnected; drop and
/// reconstruct the transport via [`I2cDevTransport::open`].
///
/// [`ScdcTransport`]: hdmi_hal::scdc::ScdcTransport
pub struct I2cDevTransport {
    bus: Mutex<LinuxI2CBus>,
}

impl I2cDevTransport {
    /// Open the given `/dev/i2c-N` device for use as an SCDC transport.
    ///
    /// The calling process must have read/write permission on the device node
    /// (typically via membership in the `i2c` group, or by running as root).
    ///
    /// # Errors
    ///
    /// Returns [`I2cDevError::DeviceOpenFailed`] if the device cannot be opened.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, I2cDevError> {
        let path = path.as_ref();
        let bus = LinuxI2CBus::new(path).map_err(|e| I2cDevError::DeviceOpenFailed {
            path: path.to_path_buf(),
            source: e.into(),
        })?;
        Ok(Self {
            bus: Mutex::new(bus),
        })
    }
}
