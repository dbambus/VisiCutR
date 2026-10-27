//! Sandboxed JavaScript for LaserScript files and parametric SVG expressions.
//!
//! VisiCut runs both through Rhino. Here the pure-Rust engine Boa evaluates
//! them on a separate thread. Boa's core has no file, network or process
//! access; only the functions registered by the caller are reachable. Every
//! run gets a wall-clock limit (checked while the script executes), a loop
//! iteration limit per function call and a recursion limit. Callers bound the
//! size of what a script may produce.
//!
//! Boa has no memory limit and cannot be stopped inside built-in functions,
//! and a failed allocation aborts the whole process. Once the application
//! names its own executable ([`use_worker`]), every script therefore runs in
//! a child process (`visicutr --script-worker`) that is killed after the time
//! limit and, on Linux, may not use more than [`WORKER_MEMORY`]: a hostile
//! file can then only end that process.
use super::{laserscript, parametric};
use crate::{i18n::tr, tr};
use boa_engine::{Context, JsError, JsValue, Script, Source};
use std::future::Future;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, mpsc};
use std::task::Poll;
use std::time::{Duration, Instant};

/// Wall-clock limit of one import's scripts.
pub const TIME_LIMIT: Duration = Duration::from_secs(5);
/// Loop iterations per function call; ends simple endless loops even where
/// the clock is not checked (inside callbacks of built-in functions).
const LOOP_LIMIT: u64 = 20_000_000;
const RECURSION_LIMIT: usize = 400;
/// VM "cycles" between two clock checks.
const BUDGET: u32 = 20_000;
/// Extra time the caller waits for a worker that does not stop by itself.
const GRACE: Duration = Duration::from_secs(1);
/// Command-line flag of the worker process.
pub const WORKER_FLAG: &str = "--script-worker";
/// Address space of the worker process (Linux).
pub const WORKER_MEMORY: u64 = 2 * 1024 * 1024 * 1024;
/// Largest answer of the worker (a drawing or an SVG of at most 20 MB).
const MAX_WORKER_OUTPUT: u64 = 64 * 1024 * 1024;

static WORKER: OnceLock<PathBuf> = OnceLock::new();

/// Runs scripts from now on in child processes of `executable`, which must
/// call [`serve_worker`] when started with [`WORKER_FLAG`].
pub fn use_worker(executable: PathBuf) {
    let _ = WORKER.set(executable);
}

/// A script run, as sent to the worker process.
#[derive(serde::Serialize, serde::Deserialize)]
pub(super) enum Job {
    LaserScript {
        source: String,
        max_points: usize,
    },
    Parametric {
        source: String,
        psvg: bool,
        saved: Vec<(String, parametric::Value)>,
    },
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) enum Output {
    Drawing(laserscript::Drawing),
    Rendered(parametric::Rendered),
}

impl Job {
    fn run_here(self, limit: Duration) -> Result<Output, String> {
        match self {
            Job::LaserScript { source, max_points } => {
                laserscript::run_here(source, limit, max_points).map(Output::Drawing)
            }
            Job::Parametric {
                source,
                psvg,
                saved,
            } => parametric::render_here(source, psvg, saved, limit).map(Output::Rendered),
        }
    }

    /// In the worker process if there is one, else on a thread of this one.
    pub(super) fn run(self, limit: Duration) -> Result<Output, String> {
        match WORKER.get() {
            Some(executable) => run_in_worker(executable, &self, limit),
            None => self.run_here(limit),
        }
    }
}

/// Entry point of the worker process: reads one job from stdin, writes the
/// result to stdout. Returns the exit code.
pub fn serve_worker() -> i32 {
    let mut input = Vec::new();
    if std::io::stdin().read_to_end(&mut input).is_err() {
        return 2;
    }
    let Ok((job, limit_ms)) = serde_json::from_slice::<(Job, u64)>(&input) else {
        return 2;
    };
    drop(input);
    let result = job.run_here(Duration::from_millis(limit_ms));
    let Ok(output) = serde_json::to_vec(&result) else {
        return 2;
    };
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(&output)
        .and_then(|()| stdout.flush())
        .is_err()
    {
        return 2;
    }
    0
}

fn run_in_worker(executable: &Path, job: &Job, limit: Duration) -> Result<Output, String> {
    let failed =
        |e: std::io::Error| tr!("Skript konnte nicht gestartet werden: {error}", error = e);
    let input = serde_json::to_vec(&(job, limit.as_millis() as u64)).map_err(|e| e.to_string())?;
    let mut command = std::process::Command::new(executable);
    command
        .arg(WORKER_FLAG)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setrlimit is async-signal-safe and touches no memory of
        // the parent.
        unsafe {
            command.pre_exec(|| {
                let limit = libc::rlimit {
                    rlim_cur: WORKER_MEMORY as libc::rlim_t,
                    rlim_max: WORKER_MEMORY as libc::rlim_t,
                };
                if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command.spawn().map_err(failed)?;
    let mut stdout = child.stdout.take().ok_or("stdout")?;
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        let read = (&mut stdout)
            .take(MAX_WORKER_OUTPUT + 1)
            .read_to_end(&mut output);
        read.map(|_| output)
    });
    // The worker reads everything before it starts; a worker that dies
    // early only makes this write fail.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&input);
    }
    let deadline = Instant::now() + limit + 2 * GRACE;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() > deadline => break None,
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.to_string());
            }
        }
    };
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(timeout_message(limit));
    };
    let output = reader
        .join()
        .ok()
        .and_then(Result::ok)
        .filter(|o| o.len() as u64 <= MAX_WORKER_OUTPUT);
    match output {
        Some(output) if status.success() => {
            serde_json::from_slice::<Result<Output, String>>(&output)
                .map_err(|_| String::from(tr("Skriptausführung ist unerwartet abgebrochen")))?
        }
        _ => Err(tr("Skript abgebrochen: zu viel Speicher belegt oder unerwartet beendet").into()),
    }
}

pub struct Engine {
    pub context: Context,
    deadline: Instant,
    limit: Duration,
    aborted: bool,
}

impl Engine {
    fn new(limit: Duration) -> Self {
        let mut context = Context::default();
        let limits = context.runtime_limits_mut();
        limits.set_loop_iteration_limit(LOOP_LIMIT);
        limits.set_recursion_limit(RECURSION_LIMIT);
        Self {
            context,
            deadline: Instant::now() + limit,
            limit,
            aborted: false,
        }
    }

    /// Runs `code` as a script and returns its completion value.
    pub fn eval(&mut self, code: &str) -> Result<JsValue, String> {
        if self.aborted || Instant::now() > self.deadline {
            self.aborted = true;
            return Err(timeout_message(self.limit));
        }
        let script = Script::parse(Source::from_bytes(code), None, &mut self.context)
            .map_err(|e| describe(&e))?;
        let deadline = self.deadline;
        let result = {
            let future = script.evaluate_async_with_budget(&mut self.context, BUDGET);
            let mut future = std::pin::pin!(future);
            let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
            loop {
                match future.as_mut().poll(&mut cx) {
                    Poll::Ready(result) => break Some(result),
                    Poll::Pending if Instant::now() > deadline => break None,
                    Poll::Pending => {}
                }
            }
        };
        match result {
            Some(result) => result.map_err(|e| describe(&e)),
            None => {
                // The interrupted script left its frame on the VM stack.
                self.aborted = true;
                Err(timeout_message(self.limit))
            }
        }
    }

    /// Converts `value` like JavaScript's `String(value)`.
    pub fn string(&mut self, value: &JsValue) -> Result<String, String> {
        value
            .to_string(&mut self.context)
            .map(|s| s.to_std_string_escaped())
            .map_err(|e| describe(&e))
    }
}

/// Runs `job` with a fresh engine on its own thread and waits at most
/// `limit` (plus a grace period) for the result.
pub fn run_isolated<R, F>(limit: Duration, job: F) -> Result<R, String>
where
    R: Send + 'static,
    F: FnOnce(&mut Engine) -> Result<R, String> + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("visicut-script".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut engine = Engine::new(limit);
            let _ = sender.send(job(&mut engine));
        })
        .map_err(|e| tr!("Skript konnte nicht gestartet werden: {error}", error = e))?;
    match receiver.recv_timeout(limit + GRACE) {
        Ok(result) => result,
        // The worker is left behind; Boa cannot be interrupted from outside.
        Err(mpsc::RecvTimeoutError::Timeout) => Err(timeout_message(limit)),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(tr("Skriptausführung ist unerwartet abgebrochen").into())
        }
    }
}

pub fn timeout_message(limit: Duration) -> String {
    tr!(
        "Skript nach {seconds} s abgebrochen (Endlosschleife oder zu aufwendig)",
        seconds = limit.as_secs_f32()
    )
}

pub fn describe(error: &JsError) -> String {
    if error.as_native().is_some_and(|e| e.is_runtime_limit()) {
        return tr!(
            "Skript abgebrochen: Laufzeitgrenze überschritten ({error})",
            error = error
        );
    }
    tr!("Skriptfehler: {error}", error = error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_javascript() {
        let value = run_isolated(TIME_LIMIT, |engine| {
            let value = engine.eval("var a = 2; a * 21")?;
            engine.string(&value)
        })
        .unwrap();
        assert_eq!(value, "42");
    }

    #[test]
    fn stops_endless_loops_by_time() {
        let error = run_isolated(Duration::from_millis(300), |engine| {
            engine
                .eval("var i = 0; while (true) { i = (i + 1) % 7; }")
                .map(|_| ())
        })
        .unwrap_err();
        assert!(error.contains("abgebrochen"), "{error}");
    }

    #[test]
    fn stops_endless_recursion() {
        let error = run_isolated(TIME_LIMIT, |engine| {
            engine.eval("function f() { return f(); } f()").map(|_| ())
        })
        .unwrap_err();
        assert!(error.contains("Skript"), "{error}");
    }

    #[test]
    fn has_no_host_access() {
        let value = run_isolated(TIME_LIMIT, |engine| {
            let value = engine.eval(
                "[typeof require, typeof fetch, typeof XMLHttpRequest, typeof process, typeof java, typeof Packages].join()",
            )?;
            engine.string(&value)
        })
        .unwrap();
        assert!(value.split(',').all(|t| t == "undefined"), "{value}");
    }
}
