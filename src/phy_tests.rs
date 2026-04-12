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
