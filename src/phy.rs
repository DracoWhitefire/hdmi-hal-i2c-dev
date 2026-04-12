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

#[cfg(test)]
mod tests {
    use display_types::cea861::hdmi_forum::HdmiForumFrl;

    use super::*;

    #[test]
    fn new_constructs_stub() {
        // Verifies that `StubPhy::new` is callable and that the resulting stub
        // accepts a PHY call without panicking.
        let mut phy = StubPhy::new(|_| {});
        phy.set_scrambling(false).unwrap();
    }

    #[test]
    fn set_frl_rate_forwards_correct_variant() {
        let mut received = None;
        {
            let mut phy = StubPhy::new(|call| received = Some(call));
            phy.set_frl_rate(HdmiForumFrl::Rate6Gbps4Lanes).unwrap();
        }
        assert!(matches!(
            received,
            Some(PhyCall::SetFrlRate(HdmiForumFrl::Rate6Gbps4Lanes))
        ));
    }

    #[test]
    fn send_ltp_forwards_correct_variant() {
        let mut received = None;
        {
            let mut phy = StubPhy::new(|call| received = Some(call));
            phy.send_ltp(LtpPattern::new(2)).unwrap();
        }
        assert!(matches!(
            received,
            Some(PhyCall::SendLtp(p)) if p.value() == 2
        ));
    }

    #[test]
    fn adjust_equalization_forwards_correct_variant() {
        let mut received = None;
        {
            let mut phy = StubPhy::new(|call| received = Some(call));
            phy.adjust_equalization(EqParams::new()).unwrap();
        }
        assert!(matches!(received, Some(PhyCall::AdjustEqualization(_))));
    }

    #[test]
    fn set_scrambling_true_forwards_correct_variant() {
        let mut received = None;
        {
            let mut phy = StubPhy::new(|call| received = Some(call));
            phy.set_scrambling(true).unwrap();
        }
        assert!(matches!(received, Some(PhyCall::SetScrambling(true))));
    }

    #[test]
    fn set_scrambling_false_forwards_correct_variant() {
        let mut received = None;
        {
            let mut phy = StubPhy::new(|call| received = Some(call));
            phy.set_scrambling(false).unwrap();
        }
        assert!(matches!(received, Some(PhyCall::SetScrambling(false))));
    }

    #[test]
    fn callback_is_invoked_once_per_call() {
        let mut count = 0usize;
        {
            let mut phy = StubPhy::new(|_| count += 1);
            phy.set_frl_rate(HdmiForumFrl::NotSupported).unwrap();
            phy.send_ltp(LtpPattern::new(1)).unwrap();
            phy.adjust_equalization(EqParams::new()).unwrap();
            phy.set_scrambling(true).unwrap();
        }
        assert_eq!(count, 4);
    }

    #[test]
    fn noop_stub_does_not_panic() {
        let mut phy = StubPhy::new(|_| {});
        phy.set_frl_rate(HdmiForumFrl::Rate3Gbps3Lanes).unwrap();
        phy.send_ltp(LtpPattern::new(3)).unwrap();
        phy.adjust_equalization(EqParams::new()).unwrap();
        phy.set_scrambling(false).unwrap();
    }
}
