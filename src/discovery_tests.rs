use std::fs;
use std::os::unix::fs::symlink;

use tempfile::TempDir;

use super::*;

/// Build a minimal synthetic sysfs tree under `tmp`:
///
/// ```text
/// <tmp>/
///   <connector>/
///     ddc -> <symlink_target>   (only created when `target` is Some)
/// ```
fn make_sysfs(connector: &str, target: Option<&str>) -> TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let connector_dir = tmp.path().join(connector);
    fs::create_dir_all(&connector_dir).unwrap();
    if let Some(t) = target {
        symlink(t, connector_dir.join("ddc")).unwrap();
    }
    tmp
}

#[test]
fn resolves_valid_connector() {
    let tmp = make_sysfs("card0-HDMI-A-1", Some("../../devices/pci0000:00/i2c-6"));
    let path = connector_ddc_adapter_in(tmp.path(), "card0-HDMI-A-1").unwrap();
    assert_eq!(path, PathBuf::from("/dev/i2c-6"));
}

#[test]
fn connector_not_found() {
    let tmp = make_sysfs("card0-HDMI-A-1", None);
    let err = connector_ddc_adapter_in(tmp.path(), "card0-HDMI-A-2").unwrap_err();
    assert!(
        matches!(err, I2cDevError::ConnectorNotFound { connector } if connector == "card0-HDMI-A-2")
    );
}

#[test]
fn connector_has_no_ddc_adapter() {
    // Connector directory exists but has no `ddc` symlink.
    let tmp = make_sysfs("card0-HDMI-A-1", None);
    let err = connector_ddc_adapter_in(tmp.path(), "card0-HDMI-A-1").unwrap_err();
    assert!(
        matches!(err, I2cDevError::ConnectorHasNoDdcAdapter { connector } if connector == "card0-HDMI-A-1")
    );
}

#[test]
fn ddc_adapter_index_unparseable_wrong_prefix() {
    // Symlink target's final component does not start with "i2c-".
    let tmp = make_sysfs("card0-HDMI-A-1", Some("../../devices/pci0000:00/usb-6"));
    let err = connector_ddc_adapter_in(tmp.path(), "card0-HDMI-A-1").unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::DdcAdapterIndexUnparseable { .. }
    ));
}

#[test]
fn ddc_adapter_index_unparseable_non_numeric_suffix() {
    // "i2c-" prefix present but suffix is not all digits.
    let tmp = make_sysfs("card0-HDMI-A-1", Some("../../devices/i2c-abc"));
    let err = connector_ddc_adapter_in(tmp.path(), "card0-HDMI-A-1").unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::DdcAdapterIndexUnparseable { .. }
    ));
}

#[test]
fn ddc_adapter_index_unparseable_empty_suffix() {
    // "i2c-" with nothing after it.
    let tmp = make_sysfs("card0-HDMI-A-1", Some("../../devices/i2c-"));
    let err = connector_ddc_adapter_in(tmp.path(), "card0-HDMI-A-1").unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::DdcAdapterIndexUnparseable { .. }
    ));
}

#[test]
fn multi_digit_adapter_index() {
    let tmp = make_sysfs("card1-HDMI-A-2", Some("../../../i2c-12"));
    let path = connector_ddc_adapter_in(tmp.path(), "card1-HDMI-A-2").unwrap();
    assert_eq!(path, PathBuf::from("/dev/i2c-12"));
}

#[test]
fn public_entry_point_returns_connector_not_found() {
    // Exercises the public `connector_ddc_adapter` function (which hardcodes
    // `/sys/class/drm`) against the live sysfs. No real system will have a
    // connector with this name, so `ConnectorNotFound` is certain.
    let err = connector_ddc_adapter("card99-HDMI-NOEXIST-99").unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::ConnectorNotFound { connector } if connector == "card99-HDMI-NOEXIST-99"
    ));
}

#[test]
fn ddc_adapter_index_unparseable_dotdot_target() {
    // Symlink target whose final component is `..` — `file_name()` returns
    // `None` for paths ending in `..`, hitting the first `ok_or_else` branch.
    let tmp = make_sysfs("card0-HDMI-A-1", Some("../.."));
    let err = connector_ddc_adapter_in(tmp.path(), "card0-HDMI-A-1").unwrap_err();
    assert!(matches!(
        err,
        I2cDevError::DdcAdapterIndexUnparseable { .. }
    ));
}
