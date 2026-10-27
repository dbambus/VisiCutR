//! Scripts from files run in a child process of the application
//! (`visicutr --script-worker`), so a hostile script can only end that
//! process: it is killed after the time limit and, on Linux, limited in
//! memory.

use std::path::PathBuf;
use std::time::{Duration, Instant};
use visicut_core::import;

fn laserscript(name: &str, source: &str) -> PathBuf {
    import::use_script_worker(env!("CARGO_BIN_EXE_visicutr").into());
    let dir = std::env::temp_dir().join(format!("visicutr-worker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    path
}

#[test]
fn scripts_run_in_the_worker_process() {
    let path = laserscript("square.ls", "move(0, 0); line(10, 0); line(10, 10);");
    let imported = import::read_file(&path).unwrap();
    assert!(imported.svg.contains("<path"), "{}", imported.svg);
}

#[test]
fn work_inside_built_in_functions_is_stopped_at_the_time_limit() {
    // Inside a built-in the engine does not check the clock: in-process
    // this ran for over ten seconds and kept a thread busy afterwards.
    let path = laserscript(
        "fill.ls",
        "var a = []; for (var i = 0; i < 50; i++) { a.push(new Array(3e7).fill(1.5)); a.pop(); }",
    );
    let start = Instant::now();
    let error = import::read_file(&path).err().expect("stopped");
    assert!(
        start.elapsed() < Duration::from_secs(9),
        "{:?}",
        start.elapsed()
    );
    assert!(error.contains("abgebrochen"), "{error}");
}

#[cfg(target_os = "linux")]
#[test]
fn memory_bombs_end_only_the_worker() {
    // In-process this aborted the application ("memory allocation of
    // 17179869184 bytes failed").
    for (name, source) in [
        (
            "repeat.ls",
            "var s = 'a'.repeat(Math.pow(2, 30)); line(s.length, 0);",
        ),
        ("double.ls", "var s = 'ab'; while (true) { s = s + s; }"),
    ] {
        let path = laserscript(name, source);
        let error = import::read_file(&path).err().expect(name);
        assert!(error.contains("abgebrochen"), "{name}: {error}");
    }
}
