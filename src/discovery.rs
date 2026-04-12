//! DRM connector to DDC adapter discovery via sysfs.

use std::path::{Path, PathBuf};

use crate::error::I2cDevError;

/// Resolve a DRM connector name to the path of its DDC I²C adapter device.
///
/// `connector` is the connector name as it appears under `/sys/class/drm/`,
/// for example `"card0-HDMI-A-1"`.
///
/// Returns the path to the `/dev/i2c-N` device, e.g. `/dev/i2c-6`.
///
/// # Errors
///
/// - [`I2cDevError::ConnectorNotFound`] — the connector does not appear under
///   `/sys/class/drm/`.
/// - [`I2cDevError::ConnectorHasNoDdcAdapter`] — the connector exists but has no
///   `ddc` symlink (common for non-HDMI connectors and unattached HDMI ports).
/// - [`I2cDevError::DdcAdapterIndexUnparseable`] — the `ddc` symlink target does not
///   end with `i2c-<N>` where `<N>` is a decimal integer.
pub fn connector_ddc_adapter(connector: &str) -> Result<PathBuf, I2cDevError> {
    connector_ddc_adapter_in(Path::new("/sys/class/drm"), connector)
}

/// Like [`connector_ddc_adapter`] but resolves the connector under `drm_root` instead
/// of `/sys/class/drm`. Used in unit tests to substitute a synthetic sysfs tree.
pub(crate) fn connector_ddc_adapter_in(
    drm_root: &Path,
    connector: &str,
) -> Result<PathBuf, I2cDevError> {
    let connector_dir = drm_root.join(connector);

    if !connector_dir.exists() {
        return Err(I2cDevError::ConnectorNotFound {
            connector: connector.to_string(),
        });
    }

    let ddc_path = connector_dir.join("ddc");

    let symlink_target =
        std::fs::read_link(&ddc_path).map_err(|_| I2cDevError::ConnectorHasNoDdcAdapter {
            connector: connector.to_string(),
        })?;

    // Only the final path component is load-bearing. The `../../..` prefix is
    // ignored so that changes in sysfs directory depth do not break parsing.
    let last = symlink_target
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| I2cDevError::DdcAdapterIndexUnparseable {
            symlink_target: symlink_target.clone(),
        })?;

    let index = last
        .strip_prefix("i2c-")
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
        .ok_or_else(|| I2cDevError::DdcAdapterIndexUnparseable {
            symlink_target: symlink_target.clone(),
        })?;

    Ok(PathBuf::from(format!("/dev/i2c-{index}")))
}

#[cfg(test)]
mod tests {
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
}
