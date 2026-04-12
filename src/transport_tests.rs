use super::*;

// --- I2cDevTransport::open -----------------------------------------------

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
