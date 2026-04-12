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
#[path = "discovery_tests.rs"]
mod tests;
