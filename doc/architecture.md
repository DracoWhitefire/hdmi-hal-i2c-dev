# Architecture

## Role

`hdmi-hal-i2c-dev` implements the `hdmi-hal` traits for Linux userspace, backed by
`/dev/i2c-N` via the `i2c-dev` kernel interface. It serves two distinct audiences:

**Protocol validation.** It is the hardware validation backend for the protocol crates —
the bridge from pure Rust trait implementations to a real HDMI sink on real hardware,
without requiring a kernel module. `culvert` and `plumbob` can be exercised against a real
sink by supplying an `I2cDevTransport` in place of any other `ScdcTransport`.

**Diagnostic tooling.** It is the hardware access layer for any tool that needs to inspect
or manipulate SCDC state directly — register dumps, scrambling state inspection, CED
counter monitoring, link status queries. A tool author who has no interest in concordance
or plumbob can depend on this crate alone and use `I2cDevTransport` as a raw conduit to
the SCDC register map.

`ScdcTransport` is implemented by opening the DDC adapter device for an HDMI connector
and issuing raw I²C transactions to the SCDC slave address (0x54). `HdmiPhy` is
implemented by `StubPhy`, a generic stub that accepts all PHY calls, forwards each as a
`PhyCall` event to a caller-supplied callback, and returns `Ok(())`. PHY programming from
Linux userspace has no standard kernel interface; the stub is sufficient to run the full
FRL training state machine and validate SCDC register logic against a real sink, and lets
callers observe or record the PHY call sequence without requiring hardware.

This crate is explicitly not the production path. The production implementation is a kernel
module operating through the DRM driver's `i2c_adapter`. This crate is the development,
validation, and diagnostic backend, intended for use before and alongside kernel integration.

---

## Scope

`hdmi-hal-i2c-dev` covers:

- `I2cDevTransport` — implements `ScdcTransport` over a `/dev/i2c-N` device file,
- `StubPhy<F>` — implements `HdmiPhy` with a caller-supplied callback; forwards each PHY
  call as a `PhyCall` event and returns `Ok(())`,
- `PhyCall` — structured event type describing a single PHY method invocation,
- `connector_ddc_adapter` — resolves a DRM connector name (e.g. `card0-HDMI-A-1`) to its
  DDC adapter device path (`/dev/i2c-N`) via sysfs,
- `I2cDevError` — the unified error type for I²C device operations.

The following are out of scope:

- **PHY programming** — there is no standard Linux userspace interface for HDMI PHY
  register access. `StubPhy` stubs all methods. A real PHY backend, if one is ever needed
  in userspace, is a separate crate.
- **EDID reads** — EDID is read over the DDC bus at I²C address 0x50, not 0x54. This crate
  only implements `ScdcTransport`, which operates against the SCDC slave at 0x54. EDID
  parsing belongs in `piaf`; reading the raw bytes is the concern of whatever integration
  layer calls `piaf`.
- **CEC** — unrelated to this crate's scope.
- **Multi-connector enumeration** — `connector_ddc_adapter` resolves a single named
  connector. Enumerating all connectors on a system is the caller's responsibility.
- **Hotplug detection** — this crate opens a device, uses it, and closes it. Monitoring
  for connector state changes is out of scope.

`connector_ddc_adapter` is included here for pragmatic reasons, not because it is
correctly scoped. The function resolves a DRM connector name to a device path via sysfs —
an operation with no dependency on SCDC, I²C transactions, or address 0x54. Any tool that
needs DDC bus access for any purpose (EDID reads, CEC, or anything else) requires the same
lookup. Placing it here forces those callers to depend on an SCDC implementation crate to
obtain a general utility function.

It lives here because there is currently no second consumer. When a second crate in this
stack needs DDC adapter resolution — or when `piaf` or any other crate needs to locate a
DDC bus without going through SCDC — `connector_ddc_adapter` should move to a dedicated
`linux-drm` (or similarly named) utility crate, and this crate should take a dependency on
it.

---

## Dependencies

```
hdmi-hal  ──►  hdmi-hal-i2c-dev
i2cdev    ──►  hdmi-hal-i2c-dev
```

- `hdmi-hal` — `ScdcTransport`, `HdmiPhy`, `EqParams`, `LtpPattern`
- `i2cdev` — safe Rust wrappers over the Linux `i2c-dev` kernel interface; provides the
  `I2C_RDWR` ioctl binding that backs `I2cDevTransport`. This is what allows
  `#![forbid(unsafe_code)]` to hold: all unsafe is contained inside `i2cdev`.
- `std` — file I/O, sysfs path resolution

This crate is `std`-only. It has no `no_std` or `alloc` story: its entire purpose is to
interact with the Linux kernel via the filesystem and ioctl interface.

---

## Connector and Adapter Discovery

The Linux DRM subsystem exposes the DDC I²C adapter for each connector through sysfs. For
a connector named `card0-HDMI-A-1`, the adapter is at:

```
/sys/class/drm/card0-HDMI-A-1/ddc
```

This is a symlink. Reading it yields a path relative to the sysfs entry, resolving to the
`i2c-N` adapter directory. The device node is then `/dev/i2c-N` where `N` is the adapter
index extracted from the symlink target.

**Parsing strategy.** Only the final path component of the symlink target is examined.
It must match the pattern `i2c-<N>` where `<N>` is a decimal integer; the prefix (`../..`
or otherwise) is ignored. This makes the implementation robust to changes in the number of
`..` components or the absolute sysfs path structure — only the final component is
load-bearing. If the final component does not match `i2c-<N>`, the function returns
`DdcAdapterIndexUnparseable { symlink_target }` with the full symlink target preserved for
diagnostics. The function does not panic on malformed targets.

`connector_ddc_adapter` performs this resolution:

```rust
/// Resolve a DRM connector name to the path of its DDC I²C adapter device.
///
/// `connector` is the connector name as it appears under `/sys/class/drm/`,
/// for example `"card0-HDMI-A-1"`.
///
/// Returns the path to the `/dev/i2c-N` device, e.g. `/dev/i2c-6`.
///
/// # Errors
///
/// Returns an error if the sysfs entry does not exist, the `ddc` symlink
/// cannot be read, or the symlink target does not contain a recognizable
/// adapter index.
pub fn connector_ddc_adapter(connector: &str) -> Result<PathBuf, I2cDevError>;
```

The typical call sequence to build a transport for a known connector is:

```rust
let path = connector_ddc_adapter("card0-HDMI-A-1")?;
let transport = I2cDevTransport::open(path)?;
```

**Hotplug sensitivity.** The `ddc` symlink is created by the kernel when a sink is
connected and removed when it disconnects. Calling `connector_ddc_adapter` with no sink
attached produces `ConnectorHasNoDdcAdapter` — the same error returned for a non-HDMI
port. The two conditions are indistinguishable from this crate's perspective. Callers that
need to distinguish them must monitor connector state through other means (e.g. DRM uevents)
before calling this function.

**TOCTOU.** There is an inherent window between `connector_ddc_adapter` returning a path
and `I2cDevTransport::open` consuming it. On systems with dynamic adapter numbering, the
adapter index resolved by `connector_ddc_adapter` may no longer refer to the DDC bus by
the time `open` is called — if an unrelated I²C adapter appeared or disappeared in the
interim, `/dev/i2c-N` may have been reassigned. This window cannot be closed in userspace.
Callers that require precise control over open timing — for example, a privileged parent
opening the device before dropping privileges — should open the file themselves and use
`I2cDevTransport::from_file`.

---

## Key Types

### `I2cDevTransport`

Implements `ScdcTransport` for a `/dev/i2c-N` device. On construction it opens the device
file; each `read` and `write` call issues an I²C transaction to the SCDC slave address
(0x54).

The SCDC register protocol is:
- **Write:** a single `I2C_RDWR` message — a two-byte I²C write containing the register
  address byte followed by the value byte.
- **Read:** a compound two-message `I2C_RDWR` transaction — a one-byte I²C write
  (register address) and a one-byte I²C read, issued together in a single ioctl call.

Both are issued via `LinuxI2CBus::transfer` from `i2cdev`, which passes all messages in a
single `I2C_RDWR` ioctl. The kernel holds the bus for the duration of the call, making
compound reads atomic. This is important for CED counters, which may increment between an
address write and a data read if the two were issued as separate ioctl calls.

```rust
pub struct I2cDevTransport { /* LinuxI2CBus */ }

impl I2cDevTransport {
    /// Open the given `/dev/i2c-N` device for use as an SCDC transport.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, I2cDevError>;

    /// Construct a transport from an already-open file.
    ///
    /// Intended for privilege-separation patterns where the `/dev/i2c-N` device
    /// is opened by a privileged parent process and the `File` is passed to an
    /// unprivileged child. The caller is responsible for ensuring the file refers
    /// to a valid `i2c-dev` device node.
    pub fn from_file(file: std::fs::File) -> Self;
}

impl ScdcTransport for I2cDevTransport {
    type Error = I2cDevError;

    fn read(&mut self, reg: u8) -> Result<u8, I2cDevError>;
    fn write(&mut self, reg: u8, value: u8) -> Result<(), I2cDevError>;
}
```

**Error recovery policy.** A `Transaction` error does not invalidate the transport. The
file descriptor remains open and the next call may succeed — transient bus conditions
(glitches, brief clock stretching) do not corrupt any transport state. The caller may
retry immediately.

If errors persist, the sink has most likely disconnected. There is no `is_connected()`
method and, due to the `amdgpu` driver collapsing all transaction failures to `EIO`, a
disconnect is indistinguishable from a persistent bus error at the API level. The
caller should drop the transport and reconstruct it — via `connector_ddc_adapter` +
`open`, or via `from_file` if controlling the fd lifecycle externally.

### `StubPhy<F>`

Implements `HdmiPhy` with a caller-supplied callback. Each method constructs a `PhyCall`
variant describing the invocation, passes it to the callback, and returns `Ok(())`.
The callback owns the side effect — logging, recording, asserting — and `StubPhy` itself
is unconditionally infallible.

```rust
pub struct StubPhy<F> {
    on_call: F,
}

impl<F: FnMut(PhyCall)> StubPhy<F> {
    pub fn new(on_call: F) -> Self;
}

impl<F: FnMut(PhyCall)> HdmiPhy for StubPhy<F> {
    type Error = Infallible;

    fn set_frl_rate(&mut self, rate: HdmiForumFrl) -> Result<(), Infallible>;
    fn send_ltp(&mut self, pattern: LtpPattern) -> Result<(), Infallible>;
    fn adjust_equalization(&mut self, params: EqParams) -> Result<(), Infallible>;
    fn set_scrambling(&mut self, enabled: bool) -> Result<(), Infallible>;
}
```

`type Error = Infallible` is unconditionally correct: the callback returns `()`, so
`StubPhy` cannot fail regardless of what the callback does. Callers that propagate
`TrainingError<_, Infallible>` can statically observe that PHY errors are impossible.

Typical uses:

```rust
// No-op: discard all PHY calls.
let phy = StubPhy::new(|_| {});

// Logging: print each call to stderr.
let phy = StubPhy::new(|call| eprintln!("{call:?}"));

// Test assertion: send calls to a channel for inspection.
let phy = StubPhy::new(|call| tx.send(call).unwrap());
```

### `PhyCall`

A structured description of a single `HdmiPhy` method invocation, used as the callback
argument for `StubPhy`.

```rust
#[derive(Debug)]
#[non_exhaustive]
pub enum PhyCall {
    SetFrlRate(HdmiForumFrl),
    SendLtp(LtpPattern),
    AdjustEqualization(EqParams),
    SetScrambling(bool),
}
```

`#[non_exhaustive]` is intentional: if `HdmiPhy` gains new methods, `PhyCall` gains new
variants. Downstream crates that match on `PhyCall` — test harnesses, diagnostic tools —
should not be forced to recompile and update exhaustive matches for methods they do not
care about. `StubPhy`'s own `HdmiPhy` impl is unaffected; the compiler enforces its
coverage through the trait, not through the enum.

### `I2cDevError`

The error type for all fallible operations in this crate.

```rust
#[non_exhaustive]
pub enum I2cDevError {
    /// The specified DRM connector was not found in sysfs.
    ConnectorNotFound { connector: String },
    /// The connector exists in sysfs but has no `ddc` symlink.
    /// Common for non-HDMI connectors and HDMI ports with no sink attached.
    ConnectorHasNoDdcAdapter { connector: String },
    /// The `ddc` symlink exists but its target does not contain a recognizable
    /// adapter index. This indicates an unexpected sysfs layout.
    DdcAdapterIndexUnparseable { symlink_target: PathBuf },
    /// The `/dev/i2c-N` device could not be opened.
    DeviceOpenFailed { path: PathBuf, source: io::Error },
    /// An I²C transaction failed.
    Transaction(I2cTransactionError),
}

/// The phase and kind of an I²C transaction failure.
pub struct I2cTransactionError {
    pub phase: MessagePhase,
    pub kind: I2cErrorKind,
}

/// Which message in the compound SCDC transaction failed.
pub enum MessagePhase {
    /// The write-phase message failed (register address byte, or write data).
    Write { register: u8 },
    /// The read-phase message failed (data retrieval).
    Read { register: u8 },
}

/// The class of I²C bus condition that caused the failure.
///
/// Variants follow the conventions in `Documentation/i2c/fault-codes.rst`.
/// See the note on `amdgpu` below for current hardware limitations.
#[non_exhaustive]
pub enum I2cErrorKind {
    /// The slave did not acknowledge its I²C address byte (`ENXIO`).
    AddressNack,
    /// The slave acknowledged its address but NACKed a data byte.
    DataNack,
    /// Arbitration was lost to another bus master (`EAGAIN`). Retry is appropriate.
    ArbitrationLost,
    /// The slave held SCL low beyond the adapter timeout threshold (`ETIMEDOUT`).
    ClockStretchTimeout,
    /// The bus was busy and the transaction could not be started (`EBUSY`).
    BusBusy,
    /// An unexpected START or STOP condition was observed on the bus (`EPROTO`).
    BusProtocolError,
    /// The I²C adapter was suspended or removed (`ESHUTDOWN`/`ENODEV`).
    AdapterGone,
    /// An errno that does not map to any known I²C condition.
    Unknown { errno: i32 },
}
```

---

## Interface Boundaries

### Below: Linux `i2c-dev` kernel interface

`I2cDevTransport` is backed by `ioctl(fd, I2C_RDWR, ...)` calls against an open
`/dev/i2c-N` file descriptor. The `i2c-dev` kernel module must be loaded; on most Linux
desktop systems it is either built in or loaded automatically. The calling process needs
read/write permission on the device node (typically via the `i2c` group).

The SCDC slave address (0x54) is fixed by the HDMI specification. This crate does not
expose it as a parameter.

#### errno mapping and the `amdgpu` limitation

The Linux I²C subsystem specifies a standard errno vocabulary for distinct bus conditions
(`Documentation/i2c/fault-codes.rst`): `ENXIO` for address-phase NACK, `EAGAIN` for
arbitration loss, `ETIMEDOUT` for timeout, `EBUSY` for bus-busy. `I2cErrorKind` is
designed against this specification.

In practice, the `amdgpu` DDC adapter does not follow this convention. Tracing the call
chain from `amdgpu_dm_i2c_xfer` through `dc_submit_i2c` and `dce_i2c_submit_command_hw`
shows that all `i2c_channel_operation_result` values — `NO_RESPONSE` (NACK), `TIMEOUT`,
`ENGINE_BUSY`, `OUT_NB_OF_RETRIES`, and all others — are collapsed to a boolean at each
layer boundary. `amdgpu_dm_i2c_xfer` maps `false` unconditionally to `-EIO`. Every
transaction failure surfaces as `EIO` regardless of the actual bus condition.

On `amdgpu`, `I2cTransactionError::kind` will always be `Unknown { errno: EIO }`.
The named variants (`AddressNack`, `Timeout`, etc.) are unreachable on this hardware. This
is a driver deficiency — the driver has the information internally and discards it — not a
kernel interface limitation. The richer variant set is retained because it is correct per
the kernel specification and meaningful on compliant adapters.

### Above: `hdmi-hal` traits

`I2cDevTransport` satisfies `ScdcTransport` as defined in `hdmi-hal`. `StubPhy<F>`
satisfies `HdmiPhy`. Callers construct these types and pass them — by value or by `&mut`
reference — to any API that accepts the corresponding trait. Nothing in this crate is
specific to `culvert` or `plumbob`; those crates accept any `ScdcTransport` and `HdmiPhy`
respectively.

The typical end-to-end validation setup:

```rust
let path = connector_ddc_adapter("card0-HDMI-A-1")?;
let transport = I2cDevTransport::open(path)?;
let scdc = Scdc::new(transport);                            // culvert
let phy = StubPhy::new(|call| eprintln!("{call:?}"));      // log PHY calls to stderr
let mut trainer = FrlTrainer::new(scdc, phy);               // plumbob
let outcome = trainer.train_at_rate(HdmiForumFrl::Rate6Gbps4Lanes, &TrainingConfig::default())?;
```

---

## `std`-only

This crate is unconditionally `std`. There is no `no_std` path, no `alloc` feature, and no
async companion planned. The entire purpose of this crate is filesystem and ioctl access,
which requires `std`. Any caller that needs `no_std` link training must supply their own
`ScdcTransport` implementation for their platform.

---

## Test Strategy

Testing is split into two tiers based on what each tier actually validates.

### Tier 1: no-hardware tests

**`connector_ddc_adapter`** is tested against a synthetic sysfs tree constructed in a
`tempfile` temporary directory. The test creates the expected symlink structure under a
configurable root, then calls `connector_ddc_adapter` with that root substituted for
`/sys/class/drm`. This covers:

- successful resolution of a valid connector,
- `ConnectorNotFound` when the connector directory is absent,
- `ConnectorHasNoDdcAdapter` when the connector exists but has no `ddc` symlink,
- `DdcAdapterIndexUnparseable` when the symlink target is malformed.

**`I2cErrorKind` mapping** is tested as a unit test against the internal function that
maps `LinuxI2CError` values to `I2cTransactionError`. Specific `LinuxI2CError::Errno`
values are constructed directly and the output variant is asserted. This covers every
named `I2cErrorKind` variant and the `Unknown { errno }` fallback without kernel
involvement.

No kernel involvement. Runs in CI on any Linux host.

### Tier 2: `i2c-stub` integration tests

**`I2cDevTransport`** is tested against the Linux `i2c-stub` kernel module, which
registers a fake I²C device at a specified address and exposes it as a real `/dev/i2c-N`
device node. Transactions go through the full `I2C_RDWR` ioctl path; the stub responds
to reads and records writes.

This tier validates:
- correct compound message construction for SCDC reads (write-then-read in a single ioctl),
- correct single-message construction for SCDC writes,
- slave address 0x54 is set on all messages,
- `AddressNack` is produced when a transaction targets an address not registered with
  the stub.

These tests require `i2c-stub` to be loaded (`modprobe i2c-stub`) and the calling process
to have write permission on the resulting device node. They are gated behind a feature flag
(`--features integration`) and are not run in standard CI. They are the authoritative test
for transport correctness and must be run before any release.

**Error conditions not covered:** bus errors, clock stretch timeouts, and arbitration loss
cannot be simulated via `i2c-stub`. `i2c-stub` always ACKs registered addresses; the only
error condition it can produce is a NACK on an unregistered address. The mapping from
`LinuxI2CError` to these variants is covered by the Tier 1 unit tests; exercising them
end-to-end requires real misbehaving hardware.

`i2cdev`'s `MockI2CDevice` is intentionally not used for transport tests. The mock does
not validate message structure, addresses, or flags — it would test `i2cdev`'s own mock
rather than this crate's message construction logic. The mock is appropriate for callers
of `ScdcTransport` (`culvert`, `plumbob`) where register-level behaviour matters; it is
not appropriate here.

---

## Design Principles

- **Follows `linux-embedded-hal` precedent.** Platform backends that implement
  hardware-abstraction traits for a specific OS or environment are a well-established
  pattern in the embedded Rust ecosystem. This crate applies that pattern to the HDMI stack.
- **Explicit about what it is not.** `StubPhy` does not silently pretend to configure
  hardware; its `Infallible` error type communicates its nature to the type system, and the
  crate documentation makes the non-production status prominent.
- **Extensible without forking.** Public types are designed so that downstream crates can
  extend behaviour — through callbacks, trait implementations, or wrapper types — without
  requiring changes to this crate. `#[non_exhaustive]` is applied to types that will grow
  alongside the rest of the stack (`I2cDevError`, `I2cErrorKind`, `PhyCall`), so that
  adding variants does not force dependent crates to update exhaustive matches they do not
  own.
- **Minimal scope.** This crate does exactly two things: implement `ScdcTransport` over
  `i2c-dev`, and stub `HdmiPhy`. Discovery helpers are included because they are inseparable
  from usability, not because this is a general-purpose DRM sysfs library.
- **No unsafe code.** `#![forbid(unsafe_code)]`. I²C device access is performed through
  `i2cdev`, which provides safe Rust wrappers over the `i2c-dev` kernel interface. All
  unsafe is contained within that crate.
- **Structured errors.** `I2cDevError` distinguishes connector-not-found from device
  open failure from transaction failure. A caller diagnosing a problem can act on the
  variant rather than parsing an error message.

---

## Open Items

**Multi-card support** — `connector_ddc_adapter` assumes the sysfs path structure is
`/sys/class/drm/<connector>/ddc`. Systems with multiple DRM cards are expected to follow
the same structure (e.g. `card1-HDMI-A-1`). This has not been tested on a multi-GPU system
and may require adjustment if the sysfs layout differs.

---

## Implementation Plan

> This section tracks what needs to be built. Drop it once the crate reaches its first
> release.

### Step 1 — upstream crates (feature branches)

The `ScdcTransport::read` signature change from `&mut self` to `&self` propagates to all
crates that define or implement the trait:

- **`hdmi-hal`** — change the trait definition.
- **`hdmi-hal-async`** — change the async trait definition.
- **`culvert`** — update the `ScdcTransport` implementation.
- **`culvert-async`** — update the async `ScdcTransport` implementation.

`HdmiPhy` is unaffected. Publish new versions of all four crates before releasing this
one.

### Step 2 — Cargo.toml

- Add a path dependency on the `hdmi-hal` feature branch for development.
- Add `i2cdev` as a dependency.
- Switch the `hdmi-hal` dependency to the published version before releasing this crate.

### Step 3 — Error types

Implement in `src/error.rs`:

- `I2cDevError` — top-level error enum with `#[non_exhaustive]`
- `I2cTransactionError` — struct with `phase: MessagePhase` and `kind: I2cErrorKind`
- `MessagePhase` — enum with `Write { register: u8 }` and `Read { register: u8 }`
- `I2cErrorKind` — enum with `#[non_exhaustive]`; map `errno` values from
  `LinuxI2CError::Errno` to variants per `fault-codes.rst`; fall back to
  `Unknown { errno: i32 }` for unrecognised values

### Step 4 — `connector_ddc_adapter`

Implement in `src/discovery.rs`:

- Accept a connector name, resolve `/sys/class/drm/<connector>/ddc`
- Return `ConnectorNotFound` if the connector directory is absent
- Return `ConnectorHasNoDdcAdapter` if the `ddc` symlink is absent
- Read the symlink target, extract the final path component, match against `i2c-<N>`
- Return `DdcAdapterIndexUnparseable { symlink_target }` if the pattern does not match
- Return `/dev/i2c-<N>` as a `PathBuf` on success

### Step 5 — `I2cDevTransport`

Implement in `src/transport.rs`:

- Wrap `LinuxI2CBus` in a `Mutex` to provide interior mutability, satisfying the `&self`
  signature of `ScdcTransport::read`
- `open(path)` — open the device file, construct `LinuxI2CBus`, wrap in `Mutex`
- `from_file(File)` — convert `File` to `LinuxI2CBus` via `AsRawFd`, wrap in `Mutex`
- `ScdcTransport::read` — lock the bus, issue a compound two-message `I2C_RDWR`
  transaction via `LinuxI2CBus::transfer`; map `LinuxI2CError` to `I2cTransactionError`
  with `phase: MessagePhase::Write` or `MessagePhase::Read` depending on which message
  in the batch failed (determined by the count returned from `transfer`)
- `ScdcTransport::write` — lock the bus, issue a single two-byte write message

### Step 6 — `StubPhy<F>` and `PhyCall`

Implement in `src/phy.rs`:

- `PhyCall` enum with `#[derive(Debug)]` and `#[non_exhaustive]`
- `StubPhy<F>` struct with `on_call: F`
- `StubPhy::new(on_call: F) -> Self`
- `HdmiPhy for StubPhy<F> where F: FnMut(PhyCall)` — each method constructs the
  appropriate `PhyCall` variant, calls `self.on_call`, returns `Ok(())`

### Step 7 — Tier 1 tests

In `tests/discovery.rs` (or `#[cfg(test)]` within `src/discovery.rs`):

- Use `tempfile` to construct a synthetic sysfs tree
- Test all four outcomes: success, `ConnectorNotFound`, `ConnectorHasNoDdcAdapter`,
  `DdcAdapterIndexUnparseable`

In `#[cfg(test)]` within `src/error.rs` (or `src/transport.rs`):

- Unit test the internal `LinuxI2CError` → `I2cTransactionError` mapping function
- Construct `LinuxI2CError::Errno(e)` for each errno named in `fault-codes.rst` and
  assert the expected `I2cErrorKind` variant
- Include a test for an unrecognised errno asserting `Unknown { errno }`

No kernel involvement; runs in standard CI.

### Step 8 — Tier 2 integration tests

In `tests/transport.rs`, gated behind `#[cfg(feature = "integration")]`:

- Require `i2c-stub` to be loaded and a stub device registered at address 0x54
- Test SCDC read: verify compound message construction and correct data retrieval
- Test SCDC write: verify single message construction and register update
- Test `AddressNack`: issue a transaction to an address not registered with the stub,
  verify `I2cErrorKind::AddressNack` (or `Unknown { errno: EIO }` on `amdgpu` — note
  the discrepancy if it arises)
- Document the setup steps required to run these tests in a `README` or test module
  doc comment
- Note explicitly that bus errors, timeouts, and arbitration loss are not covered;
  their mapping is validated by the Tier 1 unit tests only

### Step 9 — Cargo.toml metadata

Fill in the package metadata to match the rest of the stack:

- `description` — one-line summary
- `readme = "README.md"`
- `license = "MPL-2.0"`
- `repository` — GitHub URL once the repo is created
- `rust-version = "1.85"` — consistent with sibling crates
- `keywords` — e.g. `["hdmi", "i2c", "linux", "hal", "scdc"]`
- `categories = ["hardware-support", "embedded"]` — note: no `"no-std"` since this crate is `std`-only
- `[package.metadata.docs.rs]` block — add if docs.rs rendering requires any cfg flags

### Step 10 — `src/lib.rs` crate-level docs

Replace the cargo init placeholder with:

- Crate-level rustdoc: one-paragraph description, usage example showing the
  `connector_ddc_adapter` → `I2cDevTransport::open` → `Scdc::new` + `StubPhy::new` sequence
- `#![forbid(unsafe_code)]`
- `#![deny(missing_docs)]`

All public items in all modules must have rustdoc comments. `cargo rustdoc -- -D missing_docs`
must pass before release.

### Step 11 — Supporting files

Files present in all released crates in this stack:

- `README.md` — crate description, badges (CI, crates.io, docs.rs, license, rustc, SLSA),
  one-paragraph summary, usage example, links to `doc/architecture.md`
- `CHANGELOG.md` — Keep a Changelog format; start with an `[Unreleased]` section
- `CONTRIBUTING.md` — link to `doc/setup.md`, `doc/testing.md`, `doc/architecture.md`;
  issue and PR guidelines consistent with sibling crates
- `CODE_OF_CONDUCT.md` — copy from a sibling crate
- `LICENSE` — MPL-2.0; copy from a sibling crate
- `.coverage-baseline` — initialise to `0.00`; CI will ratchet it upward on first run

### Step 12 — `doc/` companion files

- `doc/setup.md` — build command, test command, how to load `i2c-stub` and set up a Tier 2
  test environment (modprobe, device node permissions, running with `--features integration`)
- `doc/testing.md` — testing strategy mirroring the architecture doc's Test Strategy section;
  what Tier 1 covers, what Tier 2 covers, what is intentionally not covered and why
- `doc/roadmap.md` — deferred work: `linux-drm` split for `connector_ddc_adapter` when a
  second consumer appears; multi-card sysfs validation; richer errno coverage once a
  compliant non-amdgpu adapter is available for CI

### Step 13 — GitHub Actions workflows

Three workflows, consistent with sibling crates:

**`ci.yml`** — runs on push to `main`/`develop` and on PRs:
- `cargo fmt --check`
- `cargo clippy --locked -- -D warnings`
- `cargo rustdoc --locked -- -D missing_docs`
- `cargo test --locked`
- `cargo test --locked --features integration` is **not** run in CI (requires hardware);
  document this explicitly in the workflow as a comment
- Coverage job with `cargo-llvm-cov`: measure, check against `.coverage-baseline`
  (−0.1% tolerance), ratchet baseline upward on push, open PR if baseline improves

**`audit.yml`** — runs on changes to `Cargo.toml` / `Cargo.lock`:
- `rustsec/audit-check`

**`publish.yml`** — runs on version tags (`v*.*.*`):
- Check tag is on `main`
- Full quality gate (fmt, clippy, docs, test) — same as CI
- `cargo package --locked`
- `actions/attest-build-provenance` — SLSA Build Level 2 provenance on the `.crate` file
- `cargo publish`
- Create GitHub release with the attested `.crate` attached and provenance verification
  instructions

Note: `fuzz.yml` is not added. The most fuzzable surface is the symlink path component
parser in `connector_ddc_adapter`, but the logic is simple enough that Tier 1 unit tests
cover it adequately. Revisit if the parser grows in complexity.

### Step 14 — Release

- Switch `hdmi-hal` dependency from path to published version
- Publish `hdmi-hal` with the `ScdcTransport::read` signature change
- Publish `hdmi-hal-i2c-dev`
- Drop this section from the architecture doc
