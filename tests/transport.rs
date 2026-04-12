//! Integration tests for [`I2cDevTransport`] against the Linux `i2c-stub` kernel module.
//!
//! These tests exercise the full `I2C_RDWR` ioctl path against a real kernel
//! I²C adapter. They validate compound message construction, slave address
//! correctness, and register read/write semantics — things that cannot be
//! verified without a responding kernel device.
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
//! I2C_STUB_ADAPTER=N cargo test --locked --features integration
//! ```
//!
//! # NACK test setup
//!
//! The NACK test requires a second adapter where address 0x54 is **not** registered,
//! so that a transaction to the SCDC slave produces a NACK. Load a second stub
//! instance with a different address, then set `I2C_STUB_ADAPTER_NACK`:
//!
//! ```sh
//! sudo modprobe i2c-stub chip_addr=0x55
//! sudo chmod 0666 /dev/i2c-M
//! I2C_STUB_ADAPTER=N I2C_STUB_ADAPTER_NACK=M cargo test --locked --features integration
//! ```
//!
//! # What is and is not covered
//!
//! **Covered:**
//! - Compound two-message construction for SCDC reads (write-then-read in a
//!   single `I2C_RDWR` ioctl)
//! - Single two-byte message construction for SCDC writes
//! - Slave address 0x54 is set on all messages
//! - Read/write round-trip correctness against the stub register map
//! - `AddressNack` (or `Unknown { errno: EIO }` on `amdgpu`) when the slave
//!   address is not registered
//!
//! **Not covered:**
//! - Bus errors, clock stretch timeouts, and arbitration loss — `i2c-stub`
//!   always ACKs registered addresses; these conditions require misbehaving
//!   hardware. Their errno→`I2cErrorKind` mapping is validated by the Tier 1
//!   unit tests in `src/transport.rs`.

#![cfg(feature = "integration")]

use std::path::PathBuf;

use hdmi_hal::scdc::ScdcTransport;
use hdmi_hal_i2c_dev::I2cDevError;
use hdmi_hal_i2c_dev::transport::I2cDevTransport;

/// Return the adapter path for `var`, or `None` if the env var is unset.
///
/// Tests that call this and get `None` return early — they are skipped rather
/// than failed so that the suite can be run with partial hardware available.
fn adapter_path(var: &str) -> Option<PathBuf> {
    std::env::var(var)
        .ok()
        .map(|n| PathBuf::from(format!("/dev/i2c-{n}")))
}

fn open_transport(var: &str) -> Option<I2cDevTransport> {
    let path = adapter_path(var)?;
    Some(
        I2cDevTransport::open(&path)
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

// --- NACK (unregistered address) --------------------------------------------

#[test]
fn read_produces_error_when_slave_not_registered() {
    // Uses a stub bus where 0x54 is not registered, so any transaction to the
    // SCDC address produces a NACK (ENXIO on compliant adapters; EIO on amdgpu).
    let Some(transport) = open_transport("I2C_STUB_ADAPTER_NACK") else {
        return;
    };
    let err = transport
        .read(0x00)
        .expect_err("expected error for unregistered address, got Ok");
    match err {
        I2cDevError::Transaction(ref e) => {
            eprintln!("NACK test: phase={:?} kind={:?}", e.phase, e.kind);
            // On a compliant adapter this will be AddressNack.
            // On amdgpu it will be Unknown { errno: EIO }.
            // Both are correct; the test just verifies an error is produced
            // and that it is a Transaction variant.
        }
        other => panic!("expected Transaction error, got: {other:?}"),
    }
}
