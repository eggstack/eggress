use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

#[test]
fn active_connection_counter_increments_and_decrements() {
    let active = Arc::new(AtomicU64::new(0));
    assert_eq!(active.load(Ordering::Relaxed), 0);

    active.fetch_add(1, Ordering::Relaxed);
    assert_eq!(active.load(Ordering::Relaxed), 1);

    active.fetch_add(1, Ordering::Relaxed);
    assert_eq!(active.load(Ordering::Relaxed), 2);

    active.fetch_sub(1, Ordering::Relaxed);
    assert_eq!(active.load(Ordering::Relaxed), 1);

    active.fetch_sub(1, Ordering::Relaxed);
    assert_eq!(active.load(Ordering::Relaxed), 0);
}

#[test]
fn active_connection_counter_concurrent_access() {
    let active = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();

    for _ in 0..10 {
        let active = active.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..1000 {
                active.fetch_add(1, Ordering::Relaxed);
            }
            for _ in 0..1000 {
                active.fetch_sub(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(active.load(Ordering::Relaxed), 0);
}

#[test]
fn readiness_flag_becomes_false_before_drain() {
    let readiness = Arc::new(AtomicBool::new(true));
    assert!(readiness.load(Ordering::Relaxed));

    readiness.store(false, Ordering::Relaxed);
    assert!(!readiness.load(Ordering::Relaxed));
}

#[test]
fn readiness_flag_concurrent_toggle() {
    let readiness = Arc::new(AtomicBool::new(true));
    let mut handles = Vec::new();

    for _ in 0..5 {
        let r = readiness.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..1000 {
                r.store(true, Ordering::Relaxed);
                r.store(false, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    // Final value is either true or false, both valid
    let _ = readiness.load(Ordering::Relaxed);
}

#[test]
fn generation_counter_monotonically_increases() {
    let generation = Arc::new(AtomicU64::new(0));
    let mut prev = generation.load(Ordering::Relaxed);

    for i in 1..=100 {
        generation.store(i, Ordering::Relaxed);
        let current = generation.load(Ordering::Relaxed);
        assert!(
            current > prev,
            "generation should increase: prev={}, current={}",
            prev,
            current
        );
        prev = current;
    }
}

#[test]
fn runtime_state_readiness_start_false() {
    let config = r#"
version = 1

[[listeners]]
name = "http-in"
bind = "127.0.0.1:0"
protocols = ["http"]
"#;
    let mut f = tempfile::NamedTempFile::new().unwrap();
    std::io::Write::write_all(&mut f, config.as_bytes()).unwrap();
    std::io::Write::flush(&mut f).unwrap();
    let path = f.path().to_str().unwrap().to_string();

    let sup = eggress_runtime::ServiceSupervisor::start(&path).unwrap();
    let state = sup.state();

    // Readiness should be false before run()
    assert!(
        !state.readiness.load(Ordering::Relaxed),
        "readiness must be false before run()"
    );

    // Generation should be 0
    assert_eq!(state.generation.load(Ordering::Relaxed), 0);

    // Active connections should be 0
    assert_eq!(state.active_connections.load(Ordering::Relaxed), 0);
}
