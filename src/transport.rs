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

impl ScdcTransport for I2cDevTransport {
    type Error = I2cDevError;

    /// Read one byte from the given SCDC register.
    ///
    /// Issues a compound two-message `I2C_RDWR` transaction: a one-byte write
    /// (register address) followed by a one-byte read, in a single ioctl call.
    /// The kernel holds the bus for the duration, making the read atomic.
    fn read(&self, reg: u8) -> Result<u8, I2cDevError> {
        let write_data = [reg];
        let mut read_buf = [0u8; 1];
        let mut bus = self
            .bus
            .lock()
            .expect("I2cDevTransport bus mutex was poisoned");
        let mut msgs = [
            LinuxI2CMessage::write(&write_data).with_address(SCDC_ADDRESS),
            LinuxI2CMessage::read(&mut read_buf).with_address(SCDC_ADDRESS),
        ];
        match bus.transfer(&mut msgs) {
            Ok(n) if n >= 2 => Ok(read_buf[0]),
            Ok(n) => {
                // Partial completion: the first `n` messages succeeded; message
                // `n` failed without a recoverable errno (adapter returned a
                // partial count rather than an error). This is unusual in
                // practice — most adapters return an error on any failure.
                let phase = if n == 0 {
                    MessagePhase::Write { register: reg }
                } else {
                    MessagePhase::Read { register: reg }
                };
                Err(I2cDevError::Transaction(I2cTransactionError {
                    phase,
                    kind: I2cErrorKind::Unknown { errno: 0 },
                }))
            }
            Err(e) => Err(I2cDevError::Transaction(I2cTransactionError {
                // When the ioctl returns an error we cannot determine which
                // message in the batch failed; the write phase is used because
                // the write message is first and is the most likely site of a
                // bus error (e.g. address NACK).
                phase: MessagePhase::Write { register: reg },
                kind: map_errno(to_raw_errno(e)),
            })),
        }
    }

    /// Write one byte to the given SCDC register.
    ///
    /// Issues a single two-byte `I2C_RDWR` write message containing the
    /// register address byte followed by the value byte.
    fn write(&mut self, reg: u8, value: u8) -> Result<(), I2cDevError> {
        let write_data = [reg, value];
        let mut bus = self
            .bus
            .lock()
            .expect("I2cDevTransport bus mutex was poisoned");
        let mut msgs = [LinuxI2CMessage::write(&write_data).with_address(SCDC_ADDRESS)];
        bus.transfer(&mut msgs).map(|_| ()).map_err(|e| {
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
