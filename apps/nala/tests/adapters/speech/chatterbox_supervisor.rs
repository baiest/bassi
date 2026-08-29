use nala::adapters::speech::chatterbox::supervisor::{Decision, decide};

#[test]
fn supervisor_reuses_running_server_without_spawning() {
    assert_eq!(decide(true, true), Decision::AlreadyRunning);
    // Even with autostart disabled, an already-healthy server is reused.
    assert_eq!(decide(true, false), Decision::AlreadyRunning);
}

#[test]
fn supervisor_spawns_when_unreachable_and_autostart_enabled() {
    assert_eq!(decide(false, true), Decision::Spawn);
}

#[test]
fn supervisor_errors_when_autostart_disabled_and_unreachable() {
    assert_eq!(decide(false, false), Decision::AutostartDisabled);
}
