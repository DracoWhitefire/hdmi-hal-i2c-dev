use i2cdev::linux::LinuxI2CError;

use hdmi_hal::scdc::ScdcTransport;

use super::*;

// --- Mock device ------------------------------------------------------------

/// In-memory SMBus device for unit tests, modelled on `i2c-stub`.
///
/// Holds a 256-entry register map. An optional `inject_error` field causes the
/// next operation to return the given errno via `LinuxI2CError::Errno`. As in `i2c-stub`
/// (`i2c-stub.c`, `I2C_SMBUS_I2C_BLOCK_DATA`), a block read ends at register 0xFF rather
/// than wrapping.
#[derive(Debug)]
struct MockDevice {
    registers: [u8; 256],
    inject_error: Option<i32>,
    /// The block reads issued, as (register, length): one entry per ioctl.
    block_reads: Vec<(u8, u8)>,
    /// The byte-data reads issued, by register: one entry per ioctl.
    byte_reads: Vec<u8>,
    /// If set, a byte-data read of this register fails with `inject_error`'s errno, or
    /// `EIO`, and other registers read normally.
    fail_byte_read_at: Option<u8>,
    /// If set, a block read returns this many bytes instead of the length asked for.
    block_len_override: Option<usize>,
}

impl MockDevice {
    fn new() -> Self {
        Self {
            registers: [0u8; 256],
            inject_error: None,
            block_reads: Vec::new(),
            byte_reads: Vec::new(),
            fail_byte_read_at: None,
            block_len_override: None,
        }
    }

    fn with_error(errno: i32) -> Self {
        Self {
            inject_error: Some(errno),
            ..Self::new()
        }
    }
}

impl SmbusDevice for MockDevice {
    fn smbus_read_byte_data(&mut self, reg: u8) -> Result<u8, LinuxI2CError> {
        self.byte_reads.push(reg);
        if self.fail_byte_read_at == Some(reg) {
            return Err(LinuxI2CError::Errno(libc::EIO));
        }
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

    fn smbus_read_i2c_block_data(&mut self, reg: u8, len: u8) -> Result<Vec<u8>, LinuxI2CError> {
        self.block_reads.push((reg, len));
        if let Some(errno) = self.inject_error {
            return Err(LinuxI2CError::Errno(errno));
        }
        let len = self.block_len_override.unwrap_or(len as usize);
        // `i2c-stub` truncates at 0xFF: at most `256 - reg` bytes.
        Ok(self.registers[reg as usize..]
            .iter()
            .copied()
            .take(len)
            .collect())
    }
}

/// The block reads the transport issued, one per ioctl.
fn block_reads(transport: I2cDevTransport<MockDevice>) -> Vec<(u8, u8)> {
    transport.device.into_inner().unwrap().block_reads
}

/// The mock behind the transport, to see every ioctl it was asked for.
fn into_mock(transport: I2cDevTransport<MockDevice>) -> MockDevice {
    transport.device.into_inner().unwrap()
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

#[test]
fn open_bytewise_nonexistent_path_returns_device_open_failed() {
    let path = std::path::Path::new("/dev/i2c-nonexistent-99999");
    let err = I2cDevTransport::open_bytewise(path).unwrap_err();
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
    // The phase is not checked: one ioctl does not say which message failed.
    let transport = I2cDevTransport::from_device(MockDevice::with_error(libc::ENXIO));
    let err = transport.read(0x01).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::Transaction(ref e) if matches!(e.kind, I2cErrorKind::AddressNack)
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

// --- read_block -------------------------------------------------------------

#[test]
fn read_block_reads_consecutive_registers_in_one_ioctl() {
    let mut mock = MockDevice::new();
    mock.registers[0x50..0x59].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let transport = I2cDevTransport::from_device(mock);
    let mut buf = [0u8; 9];
    transport.read_block(0x50, &mut buf).unwrap();
    assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(block_reads(transport), [(0x50, 9)]);
}

#[test]
fn read_block_of_the_maximum_length_is_one_ioctl() {
    let transport = I2cDevTransport::from_device(MockDevice::new());
    let mut buf = [0xFFu8; 32];
    transport.read_block(0x00, &mut buf).unwrap();
    assert_eq!(buf, [0u8; 32]);
    assert_eq!(block_reads(transport), [(0x00, 32)]);
}

#[test]
fn read_block_empty_buffer_issues_no_ioctl() {
    let transport = I2cDevTransport::from_device(MockDevice::with_error(libc::EIO));
    transport.read_block(0x50, &mut []).unwrap();
    assert_eq!(block_reads(transport), []);
}

#[test]
fn read_block_longer_than_the_maximum_is_refused_without_an_ioctl() {
    let transport = I2cDevTransport::from_device(MockDevice::new());
    let mut buf = [0xAAu8; 33];
    let err = transport.read_block(0x40, &mut buf).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::BlockReadTooLong {
            register: 0x40,
            len: 33,
            max: 32
        }
    ));
    assert_eq!(buf, [0xAAu8; 33]);
    assert_eq!(block_reads(transport), []);
}

#[test]
fn read_block_of_another_length_than_requested_is_an_error_not_padded() {
    let mut mock = MockDevice::new();
    mock.block_len_override = Some(1);
    let transport = I2cDevTransport::from_device(mock);
    let mut buf = [0xAAu8; 2];
    let err = transport.read_block(0x59, &mut buf).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::UnexpectedBlockLength {
            register: 0x59,
            requested: 2,
            returned: 1
        }
    ));
    assert_eq!(buf, [0xAA, 0xAA]);
}

#[test]
fn read_block_error_maps_to_transaction() {
    // The phase is not checked: one ioctl does not say which message failed.
    let transport = I2cDevTransport::from_device(MockDevice::with_error(libc::ENXIO));
    let mut buf = [0u8; 2];
    let err = transport.read_block(0x41, &mut buf).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::Transaction(ref e) if matches!(e.kind, I2cErrorKind::AddressNack)
    ));
    assert_eq!(block_reads(transport), [(0x41, 2)]);
}

#[test]
fn read_block_without_the_smbus_block_read_is_unsupported() {
    // An adapter without `I2C_FUNC_SMBUS_READ_I2C_BLOCK` fails the ioctl with
    // `EOPNOTSUPP`; there is no fallback to byte reads.
    let transport = I2cDevTransport::from_device(MockDevice::with_error(libc::EOPNOTSUPP));
    let mut buf = [0u8; 2];
    let err = transport.read_block(0x41, &mut buf).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::Transaction(ref e) if matches!(e.kind, I2cErrorKind::Unsupported)
    ));
    let mock = into_mock(transport);
    assert_eq!(mock.block_reads, [(0x41, 2)]);
    assert_eq!(mock.byte_reads, []);
}

#[test]
fn read_block_past_0xff_ends_at_0xff_as_on_i2c_stub() {
    // `i2c-stub` returns at most `256 - reg` bytes, so a 32-byte read from 0xF0 is 16.
    let transport = I2cDevTransport::from_device(MockDevice::new());
    let mut buf = [0xAAu8; 32];
    let err = transport.read_block(0xF0, &mut buf).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::UnexpectedBlockLength {
            register: 0xF0,
            requested: 32,
            returned: 16,
            ..
        }
    ));
    assert_eq!(buf, [0xAAu8; 32]);
}

// --- read_block, opened bytewise ------------------------------------------------

#[test]
fn bytewise_read_block_is_one_byte_read_per_register() {
    let mut mock = MockDevice::new();
    mock.registers[0x50..0x59].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let transport = I2cDevTransport::from_device_bytewise(mock);
    let mut buf = [0u8; 9];
    transport.read_block(0x50, &mut buf).unwrap();
    assert_eq!(buf, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let mock = into_mock(transport);
    assert_eq!(mock.byte_reads, (0x50..0x59).collect::<Vec<u8>>());
    assert_eq!(mock.block_reads, []);
}

#[test]
fn bytewise_read_block_has_no_length_limit_and_wraps_past_0xff() {
    let mut mock = MockDevice::new();
    for (i, r) in mock.registers.iter_mut().enumerate() {
        *r = i as u8;
    }
    let transport = I2cDevTransport::from_device_bytewise(mock);
    let mut buf = [0u8; 40];
    transport.read_block(0xF0, &mut buf).unwrap();
    let expected: Vec<u8> = (0..40u8).map(|i| 0xF0u8.wrapping_add(i)).collect();
    assert_eq!(buf.as_slice(), expected.as_slice());
}

#[test]
fn bytewise_read_block_stops_at_the_first_error() {
    let mut mock = MockDevice::new();
    mock.registers[0x50] = 0x11;
    mock.fail_byte_read_at = Some(0x51);
    let transport = I2cDevTransport::from_device_bytewise(mock);
    let mut buf = [0u8; 3];
    let err = transport.read_block(0x50, &mut buf).unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::Transaction(ref e)
            if matches!(e.kind, I2cErrorKind::Unknown { errno } if errno == libc::EIO)
    ));
    assert_eq!(buf, [0x11, 0, 0]);
    assert_eq!(into_mock(transport).byte_reads, [0x50, 0x51]);
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
fn eopnotsupp_maps_to_unsupported() {
    assert!(matches!(
        map_errno(libc::EOPNOTSUPP),
        I2cErrorKind::Unsupported
    ));
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
