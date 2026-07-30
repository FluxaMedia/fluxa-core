use super::super::*;

#[test]
fn engines_lock_survives_a_panic_while_held_by_another_thread() {
    // Poison the lock the same way a caught panic in a request would: a
    // thread panics while still holding the guard.
    let poisoner = std::thread::spawn(|| {
        let _guard = engines().lock().unwrap();
        panic!("simulated panic while holding the engines lock");
    });
    assert!(poisoner.join().is_err());

    // A naive `.lock().ok()` would now return None forever; lock_engines
    // must recover the guard so the store keeps working.
    let handle = create_headless_engine("{}");
    assert!(handle > 0);
    assert!(headless_engine_snapshot_json(handle).is_some());
    assert!(destroy_headless_engine(handle));
}
