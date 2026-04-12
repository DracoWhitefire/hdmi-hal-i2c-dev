//! [`HdmiPhy`] stub implementation for userspace validation.
//!
//! [`HdmiPhy`]: hdmi_hal::phy::HdmiPhy

use core::convert::Infallible;

use display_types::cea861::hdmi_forum::HdmiForumFrl;
use hdmi_hal::phy::{EqParams, HdmiPhy, LtpPattern};

/// A structured description of a single [`HdmiPhy`] method invocation.
///
/// Used as the callback argument for [`StubPhy`]. Match on variants to
/// observe or assert specific PHY calls in tests and diagnostic tools.
///
/// This enum is `#[non_exhaustive]`: if [`HdmiPhy`] gains new methods,
/// `PhyCall` gains new variants. Downstream match arms should include a
/// wildcard (`_`) so they do not need to be updated for methods they do not
/// care about.
///
/// [`HdmiPhy`]: hdmi_hal::phy::HdmiPhy
#[derive(Debug)]
#[non_exhaustive]
pub enum PhyCall {
    /// [`HdmiPhy::set_frl_rate`] was called with the given rate.
    ///
    /// [`HdmiPhy::set_frl_rate`]: hdmi_hal::phy::HdmiPhy::set_frl_rate
    SetFrlRate(HdmiForumFrl),
    /// [`HdmiPhy::send_ltp`] was called with the given pattern.
    ///
    /// [`HdmiPhy::send_ltp`]: hdmi_hal::phy::HdmiPhy::send_ltp
    SendLtp(LtpPattern),
    /// [`HdmiPhy::adjust_equalization`] was called with the given parameters.
    ///
    /// [`HdmiPhy::adjust_equalization`]: hdmi_hal::phy::HdmiPhy::adjust_equalization
    AdjustEqualization(EqParams),
    /// [`HdmiPhy::set_scrambling`] was called with the given flag.
    ///
    /// [`HdmiPhy::set_scrambling`]: hdmi_hal::phy::HdmiPhy::set_scrambling
    SetScrambling(bool),
}

/// An [`HdmiPhy`] stub that forwards each call as a [`PhyCall`] event to a
/// caller-supplied callback and unconditionally returns `Ok(())`.
///
/// `type Error = Infallible` is guaranteed by the design: the callback
/// returns `()`, so `StubPhy` cannot fail regardless of what the callback
/// does.
///
/// # Examples
///
/// No-op — discard all PHY calls:
///
/// ```
/// use hdmi_hal_i2c_dev::phy::StubPhy;
/// let _phy = StubPhy::new(|_| {});
/// ```
///
/// Logging — print each call to stderr:
///
/// ```
/// use hdmi_hal_i2c_dev::phy::StubPhy;
/// let _phy = StubPhy::new(|call| eprintln!("{call:?}"));
/// ```
///
/// [`HdmiPhy`]: hdmi_hal::phy::HdmiPhy
pub struct StubPhy<F> {
    on_call: F,
}

impl<F: FnMut(PhyCall)> StubPhy<F> {
    /// Construct a `StubPhy` that forwards each PHY call to `on_call`.
    pub fn new(on_call: F) -> Self {
        Self { on_call }
    }
}

impl<F: FnMut(PhyCall)> HdmiPhy for StubPhy<F> {
    type Error = Infallible;

    fn set_frl_rate(&mut self, rate: HdmiForumFrl) -> Result<(), Infallible> {
        (self.on_call)(PhyCall::SetFrlRate(rate));
        Ok(())
    }

    fn send_ltp(&mut self, pattern: LtpPattern) -> Result<(), Infallible> {
        (self.on_call)(PhyCall::SendLtp(pattern));
        Ok(())
    }

    fn adjust_equalization(&mut self, params: EqParams) -> Result<(), Infallible> {
        (self.on_call)(PhyCall::AdjustEqualization(params));
        Ok(())
    }

    fn set_scrambling(&mut self, enabled: bool) -> Result<(), Infallible> {
        (self.on_call)(PhyCall::SetScrambling(enabled));
        Ok(())
    }
}
