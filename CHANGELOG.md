# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Breaking changes

- **`PhyCall::SendLtp` carries `hdmi-hal`'s new `LtpPattern` enum.** `LtpPattern` is now a
  `#[non_exhaustive]` enum with the HDMI 2.1 pattern values (1 = all ones, 2 = all zeros,
  3 = Nyquist clock, 4 = DDE compliance, 5–8 = LFSR 0–3); `LtpPattern::new(u8)` is gone.
  Match on the variants (`PhyCall::SendLtp(LtpPattern::Lfsr0)` etc.).

## [0.1.0] - 2026-05-27

### Added

- `I2cDevTransport` — implements `ScdcTransport` over a `/dev/i2c-N` device node via
  SMBus byte-data ioctls (`I2C_SMBUS`) to the SCDC slave address (0x54).
- `StubPhy<F>` — implements `HdmiPhy` with a caller-supplied callback; forwards each PHY
  call as a `PhyCall` event and returns `Ok(())`. `type Error = Infallible`.
- `PhyCall` — structured event type describing a single `HdmiPhy` method invocation;
  `#[non_exhaustive]`.
- `connector_ddc_adapter` — resolves a DRM connector name (e.g. `card0-HDMI-A-1`) to its
  DDC adapter device path (`/dev/i2c-N`) via sysfs.
- `I2cDevError` — unified error type with variants for connector lookup, device open, and
  I²C transaction failures; `#[non_exhaustive]`.
- `I2cTransactionError`, `MessagePhase`, `I2cErrorKind` — structured transaction error
  types following `Documentation/i2c/fault-codes.rst`. `I2cErrorKind` is `#[non_exhaustive]`.
- Tier 1 unit tests: synthetic sysfs tree tests for `connector_ddc_adapter`; errno-mapping
  unit tests for `I2cErrorKind`.
- Tier 2 integration tests (gated behind `--features integration`): `I2cDevTransport`
  read/write correctness and NACK detection against the Linux `i2c-stub` kernel module.
