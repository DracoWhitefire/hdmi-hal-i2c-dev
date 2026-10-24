//! Integration tests for [`I2cDevTransport`] against the Linux `i2c-stub` kernel module.
//!
//! These tests exercise the SMBus byte-data and I2C block-read ioctl paths against a
//! real kernel I²C adapter. They validate register read/write semantics and error handling
//! — things that cannot be verified without a responding kernel device.
//!
//! # Setup
//!
//! **Load i2c-stub with a device at the SCDC slave address (0x54):**
//!
//! ```sh
//! sudo modprobe i2c-stub chip_addr=0x54
//! ```
//!
//! **Find the adapter number** (the `N` in `/dev/i2c-N`):
//!
//! ```sh
//! ls -lt /dev/i2c-* | head -3
//! # or: ls /sys/bus/i2c/drivers/i2c-stub/
//! ```
//!
//! **Grant permission** to the device node (or add yourself to the `i2c` group):
//!
//! ```sh
//! sudo chmod 0666 /dev/i2c-N
//! ```
//!
//! **Run the tests:**
//!
//! ```sh
//! I2C_STUB_ADAPTER=N cargo test --locked --features integration -- --nocapture
//! ```
//!
//! A test whose adapter variable is unset is skipped, not failed, and prints
//! `skipped: <variable> not set`; `--nocapture` shows it (a skipped test still reports
//! `ok`). With `I2C_STUB_REQUIRED=1` a skip fails instead: set it in CI and for the release
//! run, so that a forgotten adapter variable cannot pass for tier 2 coverage, and run each
//! pass with only its own tests (`--skip slave_not_registered` in the first,
//! `slave_not_registered` in the second):
//!
//! ```sh
//! I2C_STUB_REQUIRED=1 I2C_STUB_ADAPTER=N cargo test --locked --features integration -- --skip slave_not_registered
//! ```
//!
//! # NACK test setup
//!
//! The NACK tests need an adapter where nothing answers at 0x54. `i2c-stub` registers a
//! single adapter whatever `chip_addr` lists, and a second `modprobe` of the loaded
//! module changes nothing, so run them in a second pass with the stub reloaded at
//! another address (its adapter number, `M`, may differ from `N`):
//!
//! ```sh
//! sudo modprobe -r i2c-stub
//! sudo modprobe i2c-stub chip_addr=0x55
//! sudo chmod 0666 /dev/i2c-M
//! I2C_STUB_ADAPTER_NACK=M cargo test --locked --features integration slave_not_registered -- --nocapture
//! ```
//!
//! (with `I2C_STUB_REQUIRED=1` too, in CI and for the release run).
//!
//! Against `i2c-stub` the error's kind is `AdapterGone`: the stub returns `ENODEV` for an
//! address with no chip, where a real adapter returns `ENXIO` (`AddressNack`).
//!
//! # What is and is not covered
//!
//! **Covered:**
//! - SMBus byte-data read (`I2C_SMBUS`): combined write of register address
//!   then read of one byte, in a single ioctl
//! - SMBus byte-data write (`I2C_SMBUS`): write of register address and value
//! - SMBus I2C block read (`I2C_SMBUS_I2C_BLOCK_DATA`), `read_block`: consecutive
//!   registers in one ioctl, for 9 bytes (the CED block), 2 bytes and the 32-byte
//!   maximum; `i2c-stub` supports it by default (`I2C_FUNC_SMBUS_I2C_BLOCK`)
//! - A block read longer than 32 bytes refused before any ioctl
//! - A block read past 0xFF: `i2c-stub` ends it at 0xFF, so a 32-byte read from 0xF0 is
//!   `UnexpectedBlockLength` (16 bytes returned)
//! - A transport opened with `open_bytewise` reading a block a register at a time
//! - Slave address 0x54 is bound at device-open time
//! - Read/write round-trip correctness against the stub register map
//! - A `Transaction` error when the slave address is not registered, for a byte read and
//!   for a block read (`AdapterGone` on `i2c-stub`, which returns `ENODEV`; `AddressNack`
//!   on a compliant adapter; `Unknown { errno: EIO }` on `amdgpu`)
//!
//! **Not covered:**
//! - Bus errors, clock stretch timeouts, and arbitration loss — `i2c-stub`
//!   always ACKs registered addresses; these conditions require misbehaving
//!   hardware. Their errno→`I2cErrorKind` mapping is validated by the Tier 1
//!   unit tests in `src/transport_tests.rs`.

#![cfg(feature = "integration")]

use std::path::PathBuf;

use hdmi_hal::scdc::ScdcTransport;
use hdmi_hal_i2c_dev::I2cDevError;
use hdmi_hal_i2c_dev::transport::{self, I2cDevTransport};

/// Return the adapter path for `var`, or `None` if the env var is unset.
///
/// Tests that call this and get `None` return early — they are skipped rather
/// than failed so that the suite can be run with partial hardware available. A skip
/// prints a notice (visible with `--nocapture`). With `I2C_STUB_REQUIRED=1`, an unset
/// variable fails the test instead, so a run that should have tier 2 coverage cannot pass
/// without it.
fn adapter_path(var: &str) -> Option<PathBuf> {
    let Ok(n) = std::env::var(var) else {
        if std::env::var("I2C_STUB_REQUIRED").is_ok_and(|v| v == "1") {
            panic!("{var} not set, and I2C_STUB_REQUIRED=1: this test must run");
        }
        eprintln!("skipped: {var} not set");
        return None;
    };
    Some(PathBuf::from(format!("/dev/i2c-{n}")))
}

fn open_transport(var: &str) -> Option<I2cDevTransport> {
    let path = adapter_path(var)?;
    Some(
        I2cDevTransport::open(&path)
            .unwrap_or_else(|e| panic!("failed to open {}: {e}", path.display())),
    )
}

fn open_transport_bytewise(var: &str) -> Option<I2cDevTransport> {
    let path = adapter_path(var)?;
    Some(
        I2cDevTransport::open_bytewise(&path)
            .unwrap_or_else(|e| panic!("failed to open {}: {e}", path.display())),
    )
}

// --- Read/write correctness --------------------------------------------------

#[test]
fn read_unwritten_register_returns_zero() {
    // i2c-stub initialises all registers to 0x00. Reading register 0x01
    // (an SCDC register unlikely to be touched by other tests) should return 0.
    let Some(transport) = open_transport("I2C_STUB_ADAPTER") else {
        return;
    };
    let val = transport.read(0x01).expect("SCDC read failed");
    assert_eq!(val, 0x00, "stub registers should initialise to 0x00");
}

#[test]
fn write_returns_ok() {
    let Some(mut transport) = open_transport("I2C_STUB_ADAPTER") else {
        return;
    };
    transport.write(0x20, 0x01).expect("SCDC write failed");
}

#[test]
fn read_write_round_trip() {
    // Write a distinctive value to a register and read it back.
    let Some(mut transport) = open_transport("I2C_STUB_ADAPTER") else {
        return;
    };
    transport.write(0x30, 0xAB).expect("write failed");
    let val = transport.read(0x30).expect("read failed");
    assert_eq!(val, 0xAB, "read-back should match written value");
}

#[test]
fn multiple_registers_are_independent() {
    let Some(mut transport) = open_transport("I2C_STUB_ADAPTER") else {
        return;
    };
    transport.write(0x40, 0x11).expect("write reg 0x40 failed");
    transport.write(0x41, 0x22).expect("write reg 0x41 failed");
    assert_eq!(transport.read(0x40).expect("read reg 0x40 failed"), 0x11);
    assert_eq!(transport.read(0x41).expect("read reg 0x41 failed"), 0x22);
}

// --- Block reads (`read_block`) ---------------------------------------------
//
// Each test uses registers no other test writes, as the tests share one stub.

#[test]
fn block_read_returns_consecutive_registers() {
    // The CED block's nine registers, 0x50–0x58, in one ioctl.
    let Some(mut transport) = open_transport("I2C_STUB_ADAPTER") else {
        return;
    };
    let values = [0x34, 0x81, 0xFF, 0x00, 0xFF, 0xFF, 0x4E, 0x01, 0x80];
    for (reg, value) in (0x50..).zip(values) {
        transport.write(reg, value).expect("write failed");
    }
    let mut buf = [0u8; 9];
    transport
        .read_block(0x50, &mut buf)
        .expect("block read failed");
    assert_eq!(buf, values);
}

#[test]
fn block_read_of_two_registers() {
    let Some(mut transport) = open_transport("I2C_STUB_ADAPTER") else {
        return;
    };
    transport.write(0x60, 0x65).expect("write failed");
    transport.write(0x61, 0xF1).expect("write failed");
    let mut buf = [0u8; 2];
    transport
        .read_block(0x60, &mut buf)
        .expect("block read failed");
    assert_eq!(buf, [0x65, 0xF1]);
}

#[test]
fn block_read_of_the_maximum_length() {
    let Some(mut transport) = open_transport("I2C_STUB_ADAPTER") else {
        return;
    };
    for reg in 0xA0..0xC0u8 {
        transport.write(reg, reg ^ 0x5A).expect("write failed");
    }
    let mut buf = [0u8; transport::MAX_BLOCK_LEN];
    transport
        .read_block(0xA0, &mut buf)
        .expect("block read failed");
    let expected: Vec<u8> = (0xA0..0xC0u8).map(|reg| reg ^ 0x5A).collect();
    assert_eq!(buf.as_slice(), expected.as_slice());
}

#[test]
fn block_read_longer_than_the_maximum_is_refused() {
    let Some(transport) = open_transport("I2C_STUB_ADAPTER") else {
        return;
    };
    let mut buf = [0u8; 33];
    let err = transport
        .read_block(0x00, &mut buf)
        .expect_err("a 33-byte block read should be refused");
    assert!(
        matches!(
            err,
            I2cDevError::BlockReadTooLong {
                len: 33,
                max: 32,
                ..
            }
        ),
        "got: {err:?}"
    );
}

#[test]
fn block_read_past_0xff_ends_at_0xff_on_i2c_stub() {
    // `i2c-stub` returns at most `256 - command` bytes for an I2C block read, so a 32-byte
    // read from 0xF0 returns 16: the one end-to-end way to an `UnexpectedBlockLength`.
    let Some(transport) = open_transport("I2C_STUB_ADAPTER") else {
        return;
    };
    let mut buf = [0xAAu8; 32];
    let err = transport
        .read_block(0xF0, &mut buf)
        .expect_err("a block read past 0xFF should come back short on i2c-stub");
    assert!(
        matches!(
            err,
            I2cDevError::UnexpectedBlockLength {
                register: 0xF0,
                requested: 32,
                returned: 16,
                ..
            }
        ),
        "got: {err:?}"
    );
    assert_eq!(buf, [0xAAu8; 32], "the buffer should be left unchanged");
}

#[test]
fn bytewise_block_read_returns_consecutive_registers() {
    // `open_bytewise`: one byte-data read per register, for adapters without the block read.
    let Some(mut transport) = open_transport_bytewise("I2C_STUB_ADAPTER") else {
        return;
    };
    let values = [0x12, 0x34, 0x56, 0x78];
    for (reg, value) in (0xD0..).zip(values) {
        transport.write(reg, value).expect("write failed");
    }
    let mut buf = [0u8; 4];
    transport
        .read_block(0xD0, &mut buf)
        .expect("bytewise block read failed");
    assert_eq!(buf, values);
}

// --- NACK (unregistered address) --------------------------------------------

#[test]
fn read_produces_error_when_slave_not_registered() {
    // Uses a stub bus where 0x54 is not registered, so any transaction to the SCDC
    // address fails: ENXIO (a NACK) on compliant adapters, EIO on amdgpu, and ENODEV on
    // i2c-stub, which reports a missing chip that way.
    let Some(transport) = open_transport("I2C_STUB_ADAPTER_NACK") else {
        return;
    };
    let err = transport
        .read(0x00)
        .expect_err("expected error for unregistered address, got Ok");
    match err {
        I2cDevError::Transaction(ref e) => {
            eprintln!("NACK test: phase={:?} kind={:?}", e.phase, e.kind);
            // On a compliant adapter this will be AddressNack, on amdgpu
            // Unknown { errno: EIO }, on i2c-stub AdapterGone (ENODEV). All are
            // correct; the test just verifies an error is produced and that it is a
            // Transaction variant.
        }
        other => panic!("expected Transaction error, got: {other:?}"),
    }
}

#[test]
fn block_read_produces_error_when_slave_not_registered() {
    let Some(transport) = open_transport("I2C_STUB_ADAPTER_NACK") else {
        return;
    };
    let mut buf = [0u8; 9];
    let err = transport
        .read_block(0x50, &mut buf)
        .expect_err("expected error for unregistered address, got Ok");
    assert!(
        matches!(err, I2cDevError::Transaction(_)),
        "expected Transaction error, got: {err:?}"
    );
}
