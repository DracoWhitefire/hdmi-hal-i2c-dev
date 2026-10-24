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

/// The longest [`I2cDevTransport`] block read
/// ([`read_block`](ScdcTransport::read_block)): 32 bytes, the SMBus block maximum
/// (`I2C_SMBUS_BLOCK_MAX`). A longer read is refused, never split.
pub const MAX_BLOCK_LEN: usize = 32;

/// A narrow seam over the three SMBus operations [`I2cDevTransport`] requires.
///
/// This trait is crate-private. It exists solely to allow a lightweight mock
/// in unit tests without exposing the seam in the public API.
pub(crate) trait SmbusDevice {
    fn smbus_read_byte_data(&mut self, reg: u8) -> Result<u8, LinuxI2CError>;
    fn smbus_write_byte_data(&mut self, reg: u8, value: u8) -> Result<(), LinuxI2CError>;
    fn smbus_read_i2c_block_data(&mut self, reg: u8, len: u8) -> Result<Vec<u8>, LinuxI2CError>;
}

impl SmbusDevice for LinuxI2CDevice {
    fn smbus_read_byte_data(&mut self, reg: u8) -> Result<u8, LinuxI2CError> {
        I2CDevice::smbus_read_byte_data(self, reg)
    }

    fn smbus_write_byte_data(&mut self, reg: u8, value: u8) -> Result<(), LinuxI2CError> {
        I2CDevice::smbus_write_byte_data(self, reg, value)
    }

    fn smbus_read_i2c_block_data(&mut self, reg: u8, len: u8) -> Result<Vec<u8>, LinuxI2CError> {
        I2CDevice::smbus_read_i2c_block_data(self, reg, len)
    }
}

/// An SCDC transport backed by a `/dev/i2c-N` device node.
///
/// Implements [`ScdcTransport`] by issuing SMBus operations (`I2C_SMBUS`) to the
/// SCDC slave address (0x54) on the specified adapter: byte-data reads and writes,
/// and I²C block reads for [`read_block`](ScdcTransport::read_block). Each read is a
/// combined write-then-read in a single ioctl — the register offset written, a
/// repeated START, the bytes read — the transaction an SCDC read is on the wire.
///
/// # Block reads
///
/// `read_block` is one SMBus I2C block read (`I2C_SMBUS_I2C_BLOCK_DATA`), so a whole
/// block is one transaction. Its limit is [`MAX_BLOCK_LEN`], 32
/// bytes (`I2C_SMBUS_BLOCK_MAX`): a longer buffer is
/// [`I2cDevError::BlockReadTooLong`], before any ioctl, never split. An adapter
/// without `I2C_FUNC_SMBUS_READ_I2C_BLOCK` fails the ioctl, and that error is
/// returned; there is no fallback to one register at a time. Adapters with native
/// I²C transfers (the DRM DDC adapters of `amdgpu` and `i915`, for example) offer it
/// through the kernel's SMBus emulation, which issues it as the offset write and a
/// repeated-START read.
///
/// # Error recovery
///
/// A [`I2cDevError::Transaction`] error does not invalidate the transport. The
/// underlying file descriptor remains open and subsequent calls may succeed.
/// If errors persist, the sink has most likely disconnected; drop and
/// reconstruct the transport via [`I2cDevTransport::open`].
///
/// [`ScdcTransport`]: hdmi_hal::scdc::ScdcTransport
// `SmbusDevice` is intentionally `pub(crate)`: it is a test seam, not a
// public extension point. The bound on `D` is therefore more private than the
// struct itself, which the compiler warns about. The warning is expected and
// correct here — external callers cannot name `SmbusDevice` and therefore
// cannot instantiate `I2cDevTransport<D>` with any `D` other than the default
// `LinuxI2CDevice`. That is exactly the desired constraint.
#[allow(private_bounds)]
#[derive(Debug)]
pub struct I2cDevTransport<D: SmbusDevice = LinuxI2CDevice> {
    device: Mutex<D>,
}

impl I2cDevTransport<LinuxI2CDevice> {
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

// Same reasoning as the struct: `SmbusDevice` is `pub(crate)` by design, so
// the bound is more private than the impl block. Suppressed here for the same
// reason — this impl exists solely for tests and is not part of the public API.
#[cfg(test)]
#[allow(private_bounds)]
impl<D: SmbusDevice> I2cDevTransport<D> {
    /// Construct a transport from an already-opened device.
    ///
    /// Intended for unit tests; production code should use [`I2cDevTransport::open`].
    pub(crate) fn from_device(device: D) -> Self {
        Self {
            device: Mutex::new(device),
        }
    }
}

impl<D: SmbusDevice> ScdcTransport for I2cDevTransport<D> {
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

    /// Read `buf.len()` consecutive registers, starting at `reg`, in one transaction.
    ///
    /// Issues an SMBus I2C block read (`I2C_SMBUS`, `I2C_SMBUS_I2C_BLOCK_DATA`): the
    /// register offset written, a repeated START, then `buf.len()` bytes read, in a
    /// single ioctl. The kernel holds the bus for the duration.
    ///
    /// - An empty `buf` is `Ok` without an ioctl.
    /// - A `buf` longer than [`MAX_BLOCK_LEN`] is
    ///   [`I2cDevError::BlockReadTooLong`], without an ioctl: the read is never split.
    /// - A result of another length than requested is
    ///   [`I2cDevError::UnexpectedBlockLength`], never padded.
    /// - A failed ioctl is [`I2cDevError::Transaction`], mapped as for `read`; on any
    ///   error, `buf` is unchanged.
    fn read_block(&self, reg: u8, buf: &mut [u8]) -> Result<(), I2cDevError> {
        if buf.is_empty() {
            return Ok(());
        }
        if buf.len() > MAX_BLOCK_LEN {
            return Err(I2cDevError::BlockReadTooLong {
                register: reg,
                len: buf.len(),
                max: MAX_BLOCK_LEN,
            });
        }
        let mut device = self
            .device
            .lock()
            .expect("I2cDevTransport device mutex was poisoned");
        // `buf.len()` is at most 32 here, so it fits the `u8` the ioctl takes.
        let bytes = device
            .smbus_read_i2c_block_data(reg, buf.len() as u8)
            .map_err(|e| {
                I2cDevError::Transaction(I2cTransactionError {
                    phase: MessagePhase::Write { register: reg },
                    kind: map_errno(to_raw_errno(e)),
                })
            })?;
        if bytes.len() != buf.len() {
            return Err(I2cDevError::UnexpectedBlockLength {
                register: reg,
                requested: buf.len(),
                returned: bytes.len(),
            });
        }
        buf.copy_from_slice(&bytes);
        Ok(())
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
