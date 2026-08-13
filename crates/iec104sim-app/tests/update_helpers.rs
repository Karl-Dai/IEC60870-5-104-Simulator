use chrono::{DateTime, Duration, Utc};
use iec104sim_app_lib::update::{is_skipped, should_check};

fn ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[test]
fn should_check_when_no_prior_check() {
    assert!(should_check(
        None,
        ts("2026-04-28T10:00:00Z"),
        Duration::hours(6)
    ));
}

#[test]
fn should_skip_within_throttle_window() {
    let last = ts("2026-04-28T08:00:00Z");
    let now = ts("2026-04-28T10:00:00Z");
    assert!(!should_check(Some(last), now, Duration::hours(6)));
}

#[test]
fn should_check_after_throttle_window() {
    let last = ts("2026-04-28T03:00:00Z");
    let now = ts("2026-04-28T10:00:00Z");
    assert!(should_check(Some(last), now, Duration::hours(6)));
}

#[test]
fn skipped_when_versions_match() {
    assert!(is_skipped(Some("1.0.9"), "1.0.9"));
}

#[test]
fn not_skipped_without_a_saved_version() {
    assert!(!is_skipped(None, "1.0.9"));
}

#[test]
fn skipped_release_does_not_hide_a_newer_version() {
    assert!(!is_skipped(Some("1.0.9"), "1.0.10"));
}
