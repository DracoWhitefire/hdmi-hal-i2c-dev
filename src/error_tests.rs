#![cfg(test)]

use std::error::Error;
use std::io;
use std::path::PathBuf;

use crate::error::{I2cDevError, I2cErrorKind, I2cTransactionError, MessagePhase};

// --- I2cDevError::Display ----------------------------------------------------

#[test]
fn display_connector_not_found() {
    let e = I2cDevError::ConnectorNotFound {
        connector: "card0-HDMI-A-1".to_string(),
    };
    assert_eq!(
        e.to_string(),
        "DRM connector not found in sysfs: card0-HDMI-A-1"
    );
}

#[test]
fn display_connector_has_no_ddc_adapter() {
    let e = I2cDevError::ConnectorHasNoDdcAdapter {
        connector: "card0-HDMI-A-1".to_string(),
    };
    assert_eq!(
        e.to_string(),
        "connector card0-HDMI-A-1 has no DDC adapter (no sink attached, or not an HDMI port)"
    );
}

#[test]
fn display_ddc_adapter_index_unparseable() {
    let e = I2cDevError::DdcAdapterIndexUnparseable {
        symlink_target: PathBuf::from("../../devices/usb-6"),
    };
    assert_eq!(
        e.to_string(),
        "DDC symlink target does not contain a recognizable adapter index: ../../devices/usb-6"
    );
}

#[test]
fn display_device_open_failed() {
    let e = I2cDevError::DeviceOpenFailed {
        path: PathBuf::from("/dev/i2c-99"),
        source: io::Error::from(io::ErrorKind::PermissionDenied),
    };
    assert_eq!(e.to_string(), "failed to open I²C device: /dev/i2c-99");
}

#[test]
fn display_transaction() {
    let e = I2cDevError::Transaction(I2cTransactionError {
        phase: MessagePhase::Write { register: 0x20 },
        kind: I2cErrorKind::AddressNack,
    });
    assert!(e.to_string().contains("I²C transaction failed"), "got: {e}");
}

// --- I2cDevError::Error::source ----------------------------------------------

#[test]
fn source_device_open_failed_is_some() {
    let e = I2cDevError::DeviceOpenFailed {
        path: PathBuf::from("/dev/i2c-99"),
        source: io::Error::from(io::ErrorKind::PermissionDenied),
    };
    assert!(e.source().is_some());
}

#[test]
fn source_transaction_is_some() {
    let e = I2cDevError::Transaction(I2cTransactionError {
        phase: MessagePhase::Write { register: 0x00 },
        kind: I2cErrorKind::BusBusy,
    });
    assert!(e.source().is_some());
}

#[test]
fn source_connector_not_found_is_none() {
    let e = I2cDevError::ConnectorNotFound {
        connector: "x".to_string(),
    };
    assert!(e.source().is_none());
}

// --- I2cTransactionError::Display -------------------------------------------

#[test]
fn transaction_error_display_write_phase() {
    let e = I2cTransactionError {
        phase: MessagePhase::Write { register: 0x0f },
        kind: I2cErrorKind::AddressNack,
    };
    assert_eq!(e.to_string(), "write phase (register 0x0f): AddressNack");
}

#[test]
fn transaction_error_display_read_phase() {
    let e = I2cTransactionError {
        phase: MessagePhase::Read { register: 0xa0 },
        kind: I2cErrorKind::Unknown { errno: 5 },
    };
    assert_eq!(
        e.to_string(),
        "read phase (register 0xa0): Unknown { errno: 5 }"
    );
}

// --- I2cTransactionError::Error::source -------------------------------------

#[test]
fn transaction_error_source_is_none() {
    let e = I2cTransactionError {
        phase: MessagePhase::Write { register: 0x00 },
        kind: I2cErrorKind::AddressNack,
    };
    assert!(e.source().is_none());
}
