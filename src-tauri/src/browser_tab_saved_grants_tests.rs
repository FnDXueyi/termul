use super::*;

#[test]
fn the_report_arrives_only_with_the_last_answer() {
    let mut tally = ResetTally::new(3);
    assert_eq!(tally.record(true), None);
    assert_eq!(tally.record(false), None);
    assert_eq!(tally.record(true), Some(ClearReport { reset: 2, failed: 1 }));
}

#[test]
fn a_single_reset_reports_on_its_own_answer() {
    assert_eq!(
        ResetTally::new(1).record(true),
        Some(ClearReport { reset: 1, failed: 0 })
    );
}
