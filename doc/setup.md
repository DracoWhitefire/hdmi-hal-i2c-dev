# Development Setup

**Requirements:** Rust 1.85+ (stable), Linux. Install Rust via [rustup](https://rustup.rs/).

## Clone and build

```sh
git clone https://github.com/DracoWhitefire/hdmi-hal-i2c-dev.git
cd hdmi-hal-i2c-dev
cargo build
```

## Running checks

```sh
cargo fmt --check
cargo clippy --locked -- -D warnings
cargo rustdoc --locked -- -D missing_docs
```

## Running Tier 1 tests

No hardware or special privileges required.

```sh
cargo test --locked
```

## Running Tier 2 integration tests

Tier 2 tests exercise `I2cDevTransport` against the Linux `i2c-stub` kernel module.
They require `i2c-stub` to be loaded and the calling user to have write permission on
the resulting device node.

### Step 1 — Load `i2c-stub` with the SCDC slave address

```sh
sudo modprobe i2c-stub chip_addr=0x54
```

This creates a new `/dev/i2c-N` adapter. Find `N`:

```sh
# Most recently created i2c adapter:
ls -lt /dev/i2c-* | head -3

# Or via sysfs — look for the i2c-stub entry:
ls /sys/bus/i2c/drivers/i2c-stub/
```

### Step 2 — Grant device permission

```sh
sudo chmod 0666 /dev/i2c-N
# or: add yourself to the i2c group (requires re-login)
sudo usermod -aG i2c $USER
```

### Step 3 — Run the tests

```sh
I2C_STUB_ADAPTER=N cargo test --locked --features integration
```

### NACK test setup (optional)

The NACK test requires a second adapter where address 0x54 is **not** registered.
Load a second `i2c-stub` instance with a different address, find its adapter number `M`,
and set `I2C_STUB_ADAPTER_NACK`:

```sh
sudo modprobe i2c-stub chip_addr=0x55
sudo chmod 0666 /dev/i2c-M
I2C_STUB_ADAPTER=N I2C_STUB_ADAPTER_NACK=M cargo test --locked --features integration
```

If `I2C_STUB_ADAPTER_NACK` is not set, the NACK test is skipped rather than failed.

### Unloading the stub

```sh
sudo modprobe -r i2c-stub
```

## Coverage

```sh
cargo llvm-cov --locked
```

To check against the baseline:

```sh
cargo llvm-cov --locked | grep -oP '\d+\.\d+(?=%)' | tail -1
cat .coverage-baseline
```
