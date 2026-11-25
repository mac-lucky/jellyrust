//! The Next episode persistence seam: durable-first.
use super::*;
use crate::catalog::session::NextEpisodeMode;

struct Restore(NextEpisodeMode);
impl Drop for Restore {
    fn drop(&mut self) {
        restore_next_episode_mode(self.0);
    }
}

/// **A durable pick is live, read back by the next load.**
#[test]
fn a_durable_pick_is_live_and_persisted() {
    let _g = nj_base::testlock::serial();
    let _session = crate::catalog::session::TempSession::new("next-episode-set");
    let _restore = Restore(next_episode_mode());
    restore_next_episode_mode(NextEpisodeMode::Countdown);

    let saved = set_next_episode_mode(NextEpisodeMode::AfterCredits);
    assert!(saved);
    assert_eq!(next_episode_mode(), NextEpisodeMode::AfterCredits);
    assert_eq!(crate::catalog::session::load().next_episode_mode(), NextEpisodeMode::AfterCredits);

    // choosing the default again removes the key from the file, and is still a pick
    let saved = set_next_episode_mode(NextEpisodeMode::Countdown);
    assert!(saved);
    assert_eq!(crate::catalog::session::load().next_episode_mode(), NextEpisodeMode::Countdown);
}

/// **A failed write claims nothing**: the live value stays.
#[test]
fn a_failed_write_changes_nothing() {
    let _g = nj_base::testlock::serial();
    let _session = crate::catalog::session::TempSession::new("next-episode-failed");
    let _restore = Restore(next_episode_mode());
    restore_next_episode_mode(NextEpisodeMode::Countdown);
    // the session file's parent is a regular file, so no write can land
    let dir = std::env::temp_dir().join(format!("nativejelly-next-episode-blocked-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("blocker"), b"x").unwrap();
    crate::catalog::session::redirect_for_test(Some(dir.join("blocker").join("auth.json")));

    let saved = set_next_episode_mode(NextEpisodeMode::Off);
    crate::catalog::session::redirect_for_test(None);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!saved);
    assert_eq!(next_episode_mode(), NextEpisodeMode::Countdown);
}
