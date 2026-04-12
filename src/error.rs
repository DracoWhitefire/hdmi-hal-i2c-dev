//! Error types for all fallible operations in this crate.

use std::io;
use std::path::PathBuf;

/// The error type for all fallible operations in this crate.
#[non_exhaustive]
#[derive(Debug)]
pub enum I2cDevError {
    /// The specified DRM connector was not found in sysfs.
    ConnectorNotFound {
        /// The connector name that was looked up.
        connector: String,
    },
    /// The connector exists in sysfs but has no `ddc` symlink.
    /// Common for non-HDMI connectors and HDMI ports with no sink attached.
    ConnectorHasNoDdcAdapter {
        /// The connector name that was looked up.
        connector: String,
    },
    /// The `ddc` symlink exists but its target does not contain a recognizable
    /// adapter index. This indicates an unexpected sysfs layout.
    DdcAdapterIndexUnparseable {
        /// The full symlink target path that could not be parsed.
        symlink_target: PathBuf,
    },
    /// The `/dev/i2c-N` device could not be opened.
    DeviceOpenFailed {
        /// The device path that could not be opened.
        path: PathBuf,
        /// The underlying I/O error.
        source: io::Error,
    },
    /// An I²C transaction failed.
    Transaction(I2cTransactionError),
}

/// The phase and kind of an I²C transaction failure.
#[derive(Debug)]
pub struct I2cTransactionError {
    /// Which message in the compound transaction failed.
    pub phase: MessagePhase,
    /// The class of bus condition that caused the failure.
    pub kind: I2cErrorKind,
}

/// Which message in the compound SCDC transaction failed.
#[derive(Debug)]
pub enum MessagePhase {
    /// The write-phase message failed (register address byte, or write data).
    Write {
        /// The SCDC register address being accessed when the failure occurred.
        register: u8,
    },
    /// The read-phase message failed (data retrieval).
    Read {
        /// The SCDC register address being accessed when the failure occurred.
        register: u8,
    },
}

/// The class of I²C bus condition that caused the failure.
///
/// Variants follow the conventions in `Documentation/i2c/fault-codes.rst`.
/// See the `amdgpu` note in the crate documentation for current hardware
/// limitations.
#[non_exhaustive]
#[derive(Debug)]
pub enum I2cErrorKind {
    /// The slave did not acknowledge its I²C address byte (`ENXIO`).
    AddressNack,
    /// The slave acknowledged its address but NACKed a data byte.
    DataNack,
    /// Arbitration was lost to another bus master (`EAGAIN`). Retry is appropriate.
    ArbitrationLost,
    /// The slave held SCL low beyond the adapter timeout threshold (`ETIMEDOUT`).
    ClockStretchTimeout,
    /// The bus was busy and the transaction could not be started (`EBUSY`).
    BusBusy,
    /// An unexpected START or STOP condition was observed on the bus (`EPROTO`).
    BusProtocolError,
    /// The I²C adapter was suspended or removed (`ESHUTDOWN`/`ENODEV`).
    AdapterGone,
    /// An errno that does not map to any known I²C condition.
    Unknown {
        /// The raw OS error number.
        errno: i32,
    },
}

impl std::fmt::Display for I2cDevError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectorNotFound { connector } => {
                write!(f, "DRM connector not found in sysfs: {connector}")
            }
            Self::ConnectorHasNoDdcAdapter { connector } => {
                write!(
                    f,
                    "connector {connector} has no DDC adapter (no sink attached, or not an HDMI port)"
                )
            }
            Self::DdcAdapterIndexUnparseable { symlink_target } => {
                write!(
                    f,
                    "DDC symlink target does not contain a recognizable adapter index: {}",
                    symlink_target.display()
                )
            }
            Self::DeviceOpenFailed { path, .. } => {
                write!(f, "failed to open I²C device: {}", path.display())
            }
            Self::Transaction(e) => write!(f, "I²C transaction failed: {e}"),
        }
    }
}

impl std::error::Error for I2cDevError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DeviceOpenFailed { source, .. } => Some(source),
            Self::Transaction(e) => Some(e),
            _ => None,
        }
    }
}

impl std::fmt::Display for I2cTransactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let phase = match &self.phase {
            MessagePhase::Write { register } => format!("write phase (register 0x{register:02x})"),
            MessagePhase::Read { register } => format!("read phase (register 0x{register:02x})"),
        };
        write!(f, "{phase}: {:?}", self.kind)
    }
}

impl std::error::Error for I2cTransactionError {}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
