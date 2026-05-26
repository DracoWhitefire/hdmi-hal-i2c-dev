# Testing Strategy

Testing is split into two tiers based on what each tier validates and what it requires.

## Tier 1 — No hardware

Runs with `cargo test --locked`. No kernel module, no device node, no special
privileges. Runs in standard CI on any Linux host.

### `connector_ddc_adapter` (in `src/discovery.rs`)

Tested against a synthetic sysfs tree constructed in a `tempfile` temporary directory.
Each test creates the expected symlink structure under a configurable root and calls the
internal `connector_ddc_adapter_in` function with that root substituted for
`/sys/class/drm`.

**Covered:**
- Successful resolution of a valid connector with a realistic multi-component symlink target
- `ConnectorNotFound` when the connector directory is absent
- `ConnectorHasNoDdcAdapter` when the connector directory exists but has no `ddc` symlink
- `DdcAdapterIndexUnparseable` when the symlink target's final component does not start
  with `i2c-`
- `DdcAdapterIndexUnparseable` when the suffix after `i2c-` is not all digits
- `DdcAdapterIndexUnparseable` when the suffix is empty (`i2c-` with nothing after it)
- Multi-digit adapter indices

### `I2cErrorKind` errno mapping (in `src/transport.rs`)

Unit tests for the `map_errno` function, which maps raw errno integers to `I2cErrorKind`
variants following `Documentation/i2c/fault-codes.rst`.

**Covered:**
- `ENXIO` → `AddressNack`
- `EAGAIN` → `ArbitrationLost`
- `ETIMEDOUT` → `ClockStretchTimeout`
- `EBUSY` → `BusBusy`
- `EPROTO` → `BusProtocolError`
- `ESHUTDOWN` → `AdapterGone`
- `ENODEV` → `AdapterGone`
- `EIO` → `Unknown { errno: EIO }` (the only errno produced by `amdgpu`)
- Unrecognised errno → `Unknown { errno }`
- Zero errno → `Unknown { errno: 0 }` (produced by the partial-completion path)

## Tier 2 — `i2c-stub` integration tests

Gated behind `--features integration`. Requires the `i2c-stub` kernel module and a
device node with appropriate permissions. See [`doc/setup.md`](setup.md) for setup
instructions. **Not run in standard CI.** Must be run before any release.

These tests exercise the full `I2C_RDWR` ioctl path through `I2cDevTransport` and
validate things that the Tier 1 tests cannot: that messages are constructed correctly,
that the slave address 0x54 is set, and that the ioctl round-trip through the kernel
produces correct results.

**Covered:**
- An unwritten register reads back as `0x00` (stub initialises all registers to zero)
- A write operation returns `Ok(())`
- A value written to a register can be read back (round-trip correctness)
- Multiple registers are independent
- A transaction to an unregistered address produces a `Transaction` error (the kind will
  be `AddressNack` on compliant adapters, or `Unknown { errno: EIO }` on `amdgpu`)

**Intentionally not covered:**
- Bus errors, clock stretch timeouts, and arbitration loss — `i2c-stub` always ACKs
  registered addresses; these conditions require misbehaving real hardware. Their
  errno→`I2cErrorKind` mapping is validated by the Tier 1 unit tests.
- `connector_ddc_adapter` end-to-end against a real sysfs tree — the Tier 1 synthetic
  tests are sufficient; the sysfs parsing logic has no dependency on a live system.
- `StubPhy` — its correctness is trivially established by reading the implementation;
  it has no non-trivial logic to test.
