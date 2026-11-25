//! The Skip interval persistence seam: durable-first.
use super::*;
use crate::catalog::session::SkipInterval;

struct Restore(SkipInterval);
impl Drop for Restore {
    fn drop(&mut self) {
        restore_skip_interval(self.0);
    }
}

/// **A durable pick is live, read back by the next load.**
#[test]
fn a_durable_pick_is_live_and_persisted() {
    let _g = nj_base::testlock::serial();
    let _session = crate::catalog::session::TempSession::new("skip-interval-set");
    let _restore = Restore(skip_interval());
    restore_skip_interval(SkipInterval::Seconds10);

    let saved = set_skip_interval(SkipInterval::Seconds30);
    assert!(saved);
    assert_eq!(skip_interval(), SkipInterval::Seconds30);
    assert_eq!(crate::catalog::session::load().skip_interval(), SkipInterval::Seconds30);

    // choosing the default again removes the key from the file, and is still a pick
    let saved = set_skip_interval(SkipInterval::Seconds10);
    assert!(saved);
    assert_eq!(crate::catalog::session::load().skip_interval(), SkipInterval::Seconds10);
}

/// **A failed write claims nothing**: the live value stays.
#[test]
fn a_failed_write_changes_nothing() {
    let _g = nj_base::testlock::serial();
    let _session = crate::catalog::session::TempSession::new("skip-interval-failed");
    let _restore = Restore(skip_interval());
    restore_skip_interval(SkipInterval::Seconds10);
    // the session file's parent is a regular file, so no write can land
    let dir = std::env::temp_dir().join(format!("nativejelly-skip-interval-blocked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("blocker"), b"x").unwrap();
    crate::catalog::session::redirect_for_test(Some(dir.join("blocker").join("auth.json")));

    let saved = set_skip_interval(SkipInterval::Seconds60);
    crate::catalog::session::redirect_for_test(None);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!saved);
    assert_eq!(skip_interval(), SkipInterval::Seconds10);
}
