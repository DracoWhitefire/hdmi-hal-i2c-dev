use i2cdev::linux::LinuxI2CError;

use hdmi_hal::scdc::ScdcTransport;

use super::*;

// --- Mock device ------------------------------------------------------------

/// In-memory SMBus device for unit tests.
///
/// Holds a 256-entry register map. An optional `inject_error` field causes the
/// next operation to return the given errno via `LinuxI2CError::Errno`.
#[derive(Debug)]
struct MockDevice {
    registers: [u8; 256],
    inject_error: Option<i32>,
}

impl MockDevice {
    fn new() -> Self {
        Self {
            registers: [0u8; 256],
            inject_error: None,
        }
    }

    fn with_error(errno: i32) -> Self {
        Self {
            registers: [0u8; 256],
            inject_error: Some(errno),
        }
    }
}

impl SmbusDevice for MockDevice {
    fn smbus_read_byte_data(&mut self, reg: u8) -> Result<u8, LinuxI2CError> {
        if let Some(errno) = self.inject_error {
            return Err(LinuxI2CError::Errno(errno));
        }
        Ok(self.registers[reg as usize])
    }

    fn smbus_write_byte_data(&mut self, reg: u8, value: u8) -> Result<(), LinuxI2CError> {
        if let Some(errno) = self.inject_error {
            return Err(LinuxI2CError::Errno(errno));
        }
        self.registers[reg as usize] = value;
        Ok(())
    }
}

// --- I2cDevTransport::open --------------------------------------------------

#[test]
fn open_nonexistent_path_returns_device_open_failed() {
    let path = std::path::Path::new("/dev/i2c-nonexistent-99999");
    let err = I2cDevTransport::open(path).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::DeviceOpenFailed { path: ref p, .. } if p == path
    ));
}

// --- read -------------------------------------------------------------------

#[test]
fn read_returns_register_value() {
    let mut mock = MockDevice::new();
    mock.registers[0x10] = 0xAB;
    let transport = I2cDevTransport::from_device(mock);
    assert_eq!(transport.read(0x10).unwrap(), 0xAB);
}

#[test]
fn read_unwritten_register_returns_zero() {
    let transport = I2cDevTransport::from_device(MockDevice::new());
    assert_eq!(transport.read(0x00).unwrap(), 0x00);
}

#[test]
fn read_error_maps_to_transaction() {
    let transport = I2cDevTransport::from_device(MockDevice::with_error(libc::ENXIO));
    let err = transport.read(0x01).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::Transaction(ref e)
            if matches!(e.kind, I2cErrorKind::AddressNack)
            && matches!(e.phase, MessagePhase::Write { register: 0x01 })
    ));
}

// --- write ------------------------------------------------------------------

#[test]
fn write_stores_value_in_register() {
    let mock = MockDevice::new();
    let mut transport = I2cDevTransport::from_device(mock);
    transport.write(0x20, 0x55).unwrap();
    assert_eq!(transport.read(0x20).unwrap(), 0x55);
}

#[test]
fn write_error_maps_to_transaction() {
    let mock = MockDevice::with_error(libc::EAGAIN);
    let mut transport = I2cDevTransport::from_device(mock);
    let err = transport.write(0x02, 0xFF).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::Transaction(ref e)
            if matches!(e.kind, I2cErrorKind::ArbitrationLost)
            && matches!(e.phase, MessagePhase::Write { register: 0x02 })
    ));
}

// --- round-trip -------------------------------------------------------------

#[test]
fn read_write_round_trip() {
    let mock = MockDevice::new();
    let mut transport = I2cDevTransport::from_device(mock);
    transport.write(0x30, 0xCD).unwrap();
    assert_eq!(transport.read(0x30).unwrap(), 0xCD);
}

#[test]
fn multiple_registers_are_independent() {
    let mock = MockDevice::new();
    let mut transport = I2cDevTransport::from_device(mock);
    transport.write(0x40, 0x11).unwrap();
    transport.write(0x41, 0x22).unwrap();
    assert_eq!(transport.read(0x40).unwrap(), 0x11);
    assert_eq!(transport.read(0x41).unwrap(), 0x22);
}

// --- map_errno --------------------------------------------------------------

#[test]
fn enxio_maps_to_address_nack() {
    assert!(matches!(map_errno(libc::ENXIO), I2cErrorKind::AddressNack));
}

#[test]
fn eagain_maps_to_arbitration_lost() {
    assert!(matches!(
        map_errno(libc::EAGAIN),
        I2cErrorKind::ArbitrationLost
    ));
}

#[test]
fn etimedout_maps_to_clock_stretch_timeout() {
    assert!(matches!(
        map_errno(libc::ETIMEDOUT),
        I2cErrorKind::ClockStretchTimeout
    ));
}

#[test]
fn ebusy_maps_to_bus_busy() {
    assert!(matches!(map_errno(libc::EBUSY), I2cErrorKind::BusBusy));
}

#[test]
fn eproto_maps_to_bus_protocol_error() {
    assert!(matches!(
        map_errno(libc::EPROTO),
        I2cErrorKind::BusProtocolError
    ));
}

#[test]
fn eshutdown_maps_to_adapter_gone() {
    assert!(matches!(
        map_errno(libc::ESHUTDOWN),
        I2cErrorKind::AdapterGone
    ));
}

#[test]
fn enodev_maps_to_adapter_gone() {
    assert!(matches!(map_errno(libc::ENODEV), I2cErrorKind::AdapterGone));
}

#[test]
fn eio_maps_to_unknown() {
    // EIO is the only errno produced by amdgpu, regardless of the actual
    // bus condition. It has no fault-codes.rst mapping.
    assert!(matches!(
        map_errno(libc::EIO),
        I2cErrorKind::Unknown { errno } if errno == libc::EIO
    ));
}

#[test]
fn unrecognised_errno_maps_to_unknown() {
    assert!(matches!(
        map_errno(9999),
        I2cErrorKind::Unknown { errno: 9999 }
    ));
}

#[test]
fn zero_errno_maps_to_unknown() {
    // Produced by partial-completion path (no errno available).
    assert!(matches!(map_errno(0), I2cErrorKind::Unknown { errno: 0 }));
}
