# hdmi-hal-i2c-dev

[![CI](https://github.com/DracoWhitefire/hdmi-hal-i2c-dev/actions/workflows/ci.yml/badge.svg)](https://github.com/DracoWhitefire/hdmi-hal-i2c-dev/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/hdmi-hal-i2c-dev.svg)](https://crates.io/crates/hdmi-hal-i2c-dev)
[![docs.rs](https://docs.rs/hdmi-hal-i2c-dev/badge.svg)](https://docs.rs/hdmi-hal-i2c-dev)
[![License: MPL-2.0](https://img.shields.io/badge/license-MPL--2.0-blue.svg)](LICENSE)
[![Rust 1.85+](https://img.shields.io/badge/rustc-1.85+-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)
[![SLSA Level 2](https://slsa.dev/images/gh-badge-level2.svg)](https://slsa.dev)

Linux userspace [`hdmi-hal`] backend backed by `/dev/i2c-N` via the `i2c-dev` kernel interface.

`hdmi-hal-i2c-dev` is the **development, validation, and diagnostic backend** for the HDMI stack.
It implements [`ScdcTransport`] over a real `/dev/i2c-N` device node and provides [`StubPhy`], a
callback-based [`HdmiPhy`] stub that lets you run the full FRL link training state machine from
userspace without a PHY kernel driver. Use it to validate protocol crates ([`culvert`], [`plumbob`])
against a real HDMI sink before and alongside kernel integration.

This crate is **not** the production path. The production implementation is a kernel module
operating through the DRM driver's `i2c_adapter`. See [`doc/architecture.md`] for full context.

## Usage

```toml
[dependencies]
hdmi-hal-i2c-dev = "0.1"
```

```rust,no_run
use hdmi_hal_i2c_dev::{connector_ddc_adapter, I2cDevTransport, StubPhy};

// Resolve the DDC adapter for a named DRM connector and open the transport.
let path = connector_ddc_adapter("card0-HDMI-A-1")?;
let transport = I2cDevTransport::open(path)?;

// StubPhy forwards each PHY call to a callback and returns Ok(()).
// Pass a closure for logging, test assertions, or recording.
let phy = StubPhy::new(|call| eprintln!("{call:?}"));

// Pass transport and phy to culvert / plumbob as you would any other backend.
# Ok::<(), hdmi_hal_i2c_dev::I2cDevError>(())
```

## Privileges

The calling process needs read/write permission on the `/dev/i2c-N` device node — typically via
membership in the `i2c` group, or by running as root.

## `amdgpu` limitation

The `amdgpu` DDC adapter collapses all I²C errors to `EIO`. On that hardware,
`I2cTransactionError::kind` is always `Unknown { errno: EIO }` regardless of the actual bus
condition. The named `I2cErrorKind` variants (`AddressNack`, `ClockStretchTimeout`, etc.) are
correct per `Documentation/i2c/fault-codes.rst` and are reachable on compliant adapters.
See [`doc/architecture.md`] for details.

## Documentation

- [`doc/architecture.md`] — design rationale, type descriptions, test strategy, and implementation plan
- [`doc/setup.md`] — build and test instructions, including `i2c-stub` setup for integration tests
- [`doc/testing.md`] — what each test tier covers and why

[`hdmi-hal`]: https://crates.io/crates/hdmi-hal
[`ScdcTransport`]: https://docs.rs/hdmi-hal/latest/hdmi_hal/scdc/trait.ScdcTransport.html
[`HdmiPhy`]: https://docs.rs/hdmi-hal/latest/hdmi_hal/phy/trait.HdmiPhy.html
[`StubPhy`]: https://docs.rs/hdmi-hal-i2c-dev/latest/hdmi_hal_i2c_dev/phy/struct.StubPhy.html
[`culvert`]: https://crates.io/crates/culvert
[`plumbob`]: https://crates.io/crates/plumbob
[`doc/architecture.md`]: doc/architecture.md
[`doc/setup.md`]: doc/setup.md
[`doc/testing.md`]: doc/testing.md
