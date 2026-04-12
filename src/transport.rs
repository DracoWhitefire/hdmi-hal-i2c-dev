//! [`ScdcTransport`] implementation backed by a `/dev/i2c-N` device node.
//!
//! [`ScdcTransport`]: hdmi_hal::scdc::ScdcTransport

use std::path::Path;
use std::sync::Mutex;

use i2cdev::core::I2CDevice;
use i2cdev::linux::{LinuxI2CDevice, LinuxI2CError};

use hdmi_hal::scdc::ScdcTransport;

use crate::error::{I2cDevError, I2cErrorKind, I2cTransactionError, MessagePhase};

/// SCDC slave address, fixed by the HDMI specification (HDMI 2.1 Table 10-3).
const SCDC_ADDRESS: u16 = 0x54;

/// An SCDC transport backed by a `/dev/i2c-N` device node.
#[derive(Debug)]
///
/// Implements [`ScdcTransport`] by issuing SMBus byte-data operations
/// (`I2C_SMBUS`) to the SCDC slave address (0x54) on the specified adapter.
/// SMBus byte-data read is a combined write-then-read in a single ioctl,
/// semantically equivalent to a raw I²C combined transaction for SCDC's
/// single-byte register model.
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
    device: Mutex<LinuxI2CDevice>,
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
        let device =
            LinuxI2CDevice::new(path, SCDC_ADDRESS).map_err(|e| I2cDevError::DeviceOpenFailed {
                path: path.to_path_buf(),
                source: e.into(),
            })?;
        Ok(Self {
            device: Mutex::new(device),
        })
    }
}

impl ScdcTransport for I2cDevTransport {
    type Error = I2cDevError;

    /// Read one byte from the given SCDC register.
    ///
    /// Issues an SMBus byte-data read (`I2C_SMBUS`): a combined write of the
    /// register address followed by a read of one byte, in a single ioctl
    /// call. The kernel holds the bus for the duration, making the read atomic.
    fn read(&self, reg: u8) -> Result<u8, I2cDevError> {
        let mut device = self
            .device
            .lock()
            .expect("I2cDevTransport device mutex was poisoned");
        device.smbus_read_byte_data(reg).map_err(|e| {
            I2cDevError::Transaction(I2cTransactionError {
                phase: MessagePhase::Write { register: reg },
                kind: map_errno(to_raw_errno(e)),
            })
        })
    }

    /// Write one byte to the given SCDC register.
    ///
    /// Issues an SMBus byte-data write (`I2C_SMBUS`): a single write of the
    /// register address byte followed by the value byte.
    fn write(&mut self, reg: u8, value: u8) -> Result<(), I2cDevError> {
        let mut device = self
            .device
            .lock()
            .expect("I2cDevTransport device mutex was poisoned");
        device.smbus_write_byte_data(reg, value).map_err(|e| {
            I2cDevError::Transaction(I2cTransactionError {
                phase: MessagePhase::Write { register: reg },
                kind: map_errno(to_raw_errno(e)),
            })
        })
    }
}

/// Extract a raw errno integer from a [`LinuxI2CError`].
fn to_raw_errno(e: LinuxI2CError) -> i32 {
    match e {
        LinuxI2CError::Errno(n) => n,
        LinuxI2CError::Io(io) => io.raw_os_error().unwrap_or(0),
    }
}

/// Map a raw errno to an [`I2cErrorKind`] variant, following the conventions
/// in `Documentation/i2c/fault-codes.rst`.
///
/// `DataNack` has no standard errno mapping and is not produced by this
/// function; it falls through to `Unknown`.
///
/// On `amdgpu`, all failures produce `EIO` regardless of the actual bus
/// condition, so `Unknown { errno: EIO }` is the only variant reachable on
/// that hardware. See the crate documentation for details.
pub(crate) fn map_errno(errno: i32) -> I2cErrorKind {
    match errno {
        libc::ENXIO => I2cErrorKind::AddressNack,
        libc::EAGAIN => I2cErrorKind::ArbitrationLost,
        libc::ETIMEDOUT => I2cErrorKind::ClockStretchTimeout,
        libc::EBUSY => I2cErrorKind::BusBusy,
        libc::EPROTO => I2cErrorKind::BusProtocolError,
        libc::ESHUTDOWN | libc::ENODEV => I2cErrorKind::AdapterGone,
        e => I2cErrorKind::Unknown { errno: e },
    }
}
#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
