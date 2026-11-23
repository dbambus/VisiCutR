//! Child process for the parts of an import that a hostile file can drive
//! beyond any limit inside a library: scripts (Boa has no memory limit and
//! does not check the clock inside built-in functions) and PDF pages (hayro
//! inflates streams without a size limit). A failed allocation aborts the
//! whole process, so once the application names its own executable
//! ([`use_executable`]) these jobs run in `visicutr --import-worker`, which is
//! killed after the time limit and may not use more than [`MEMORY`]
//! ([`WorkerAllocator`] on all platforms, on Linux also the address space).
//! A hostile file can then only end that process.
//!
//! Without an executable (unit tests) the jobs run in this process.
use super::ImportError;
use super::plf::{self, ParameterValue};
use base64::Engine as _;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// Command-line flag of the worker process.
pub const FLAG: &str = "--import-worker";
/// Memory of the worker process: heap on all platforms
/// ([`WorkerAllocator`]), address space on Linux.
pub const MEMORY: u64 = 2 * 1024 * 1024 * 1024;

/// Heap bytes the process may hold; `usize::MAX`: not counted (the
/// application itself).
static HEAP_LIMIT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(usize::MAX);
/// Heap bytes allocated since the limit was set (negative after freeing
/// memory allocated before).
static HEAP_USED: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Global allocator of the application. In the worker process ([`serve`])
/// it refuses allocations beyond [`MEMORY`]; the failed allocation ends the
/// worker instead of filling the computer's memory (Windows and macOS have
/// no address-space limit like Linux). Without a limit it only forwards to
/// the system allocator.
pub struct WorkerAllocator;

impl WorkerAllocator {
    fn reserve(size: usize) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        let limit = HEAP_LIMIT.load(Relaxed);
        if limit == usize::MAX {
            return true;
        }
        let size = size as isize;
        let used = HEAP_USED.fetch_add(size, Relaxed) + size;
        if used > limit as isize {
            HEAP_USED.fetch_sub(size, Relaxed);
            return false;
        }
        true
    }

    fn release(size: usize) {
        use std::sync::atomic::Ordering::Relaxed;
        if HEAP_LIMIT.load(Relaxed) != usize::MAX {
            HEAP_USED.fetch_sub(size as isize, Relaxed);
        }
    }
}

// SAFETY: every call is forwarded to the system allocator with the same
// layout; only the bookkeeping and the refusal (null) are added.
unsafe impl std::alloc::GlobalAlloc for WorkerAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        if !Self::reserve(layout.size()) {
            return std::ptr::null_mut();
        }
        // SAFETY: as documented for `GlobalAlloc::alloc`.
        let pointer = unsafe { std::alloc::System.alloc(layout) };
        if pointer.is_null() {
            Self::release(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        if !Self::reserve(layout.size()) {
            return std::ptr::null_mut();
        }
        // SAFETY: as documented for `GlobalAlloc::alloc_zeroed`.
        let pointer = unsafe { std::alloc::System.alloc_zeroed(layout) };
        if pointer.is_null() {
            Self::release(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: std::alloc::Layout) {
        // SAFETY: `pointer` was allocated by the system allocator with `layout`.
        unsafe { std::alloc::System.dealloc(pointer, layout) };
        Self::release(layout.size());
    }

    unsafe fn realloc(
        &self,
        pointer: *mut u8,
        layout: std::alloc::Layout,
        new_size: usize,
    ) -> *mut u8 {
        let old = layout.size();
        if new_size > old && !Self::reserve(new_size - old) {
            return std::ptr::null_mut();
        }
        // SAFETY: as documented for `GlobalAlloc::realloc`.
        let moved = unsafe { std::alloc::System.realloc(pointer, layout, new_size) };
        match (moved.is_null(), new_size > old) {
            (true, true) => Self::release(new_size - old),
            (false, false) => Self::release(old - new_size),
            _ => {}
        }
        moved
    }
}

/// Limits the heap of this process to `bytes` (see [`WorkerAllocator`]).
fn limit_heap(bytes: u64) {
    use std::sync::atomic::Ordering::Relaxed;
    HEAP_USED.store(0, Relaxed);
    HEAP_LIMIT.store(usize::try_from(bytes).unwrap_or(usize::MAX - 1), Relaxed);
}
/// Time for one PDF page (as for Ghostscript).
pub(super) const PDF_TIME_LIMIT: Duration = Duration::from_secs(60);
/// Largest answer of the worker (a drawing or an SVG of at most 20 MB).
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;
/// Extra time for starting the process and passing the data.
const GRACE: Duration = Duration::from_secs(2);

static EXECUTABLE: OnceLock<PathBuf> = OnceLock::new();

/// Runs the jobs from now on in child processes of `executable`, which
/// must call [`serve`] when started with [`FLAG`].
pub fn use_executable(executable: PathBuf) {
    let _ = EXECUTABLE.set(executable);
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) enum Job {
    LaserScript {
        source: String,
        max_points: usize,
    },
    Parametric {
        source: String,
        psvg: bool,
        saved: Vec<(String, ParameterValue)>,
    },
    /// The PDF file, base64-encoded.
    Pdf {
        data: String,
    },
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(super) enum Output {
    Drawing(plf::Drawing),
    Rendered(plf::Rendered),
    Drawn { svg: String, warnings: Vec<String> },
}

impl Job {
    pub(super) fn pdf(data: &[u8]) -> Self {
        Job::Pdf {
            data: base64::engine::general_purpose::STANDARD.encode(data),
        }
    }

    fn run_here(self, limit: Duration) -> Result<Output, ImportError> {
        match self {
            Job::LaserScript { source, max_points } => {
                plf::laserscript_here(source, limit, max_points).map(Output::Drawing)
            }
            Job::Parametric {
                source,
                psvg,
                saved,
            } => plf::parametric_here(source, psvg, saved, limit).map(Output::Rendered),
            Job::Pdf { data } => {
                let data = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .map_err(|e| ImportError::Encoding(e.to_string()))?;
                super::pdf::convert_here(data).map(|imported| Output::Drawn {
                    svg: imported.svg,
                    warnings: imported.warnings,
                })
            }
        }
    }

    /// In the worker process if there is one, else in this one.
    pub(super) fn run(self, limit: Duration) -> Result<Output, ImportError> {
        let limit = super::time_left(limit)?;
        match EXECUTABLE.get() {
            Some(executable) => run_in_worker(executable, &self, limit),
            None => self.run_here(limit),
        }
    }
}

/// Entry point of the worker process: reads one job from stdin, writes the
/// result to stdout. Returns the exit code.
pub fn serve() -> i32 {
    limit_heap(MEMORY);
    let mut input = Vec::new();
    if std::io::stdin().read_to_end(&mut input).is_err() {
        return 2;
    }
    let Ok((job, limit_ms)) = serde_json::from_slice::<(Job, u64)>(&input) else {
        return 2;
    };
    drop(input);
    // Ends on its own, even if the application died and nobody kills it.
    let lifetime = Duration::from_millis(limit_ms) + 2 * GRACE;
    std::thread::spawn(move || {
        std::thread::sleep(lifetime);
        std::process::exit(3);
    });
    let result = job
        .run_here(Duration::from_millis(limit_ms))
        .map_err(String::from);
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

fn run_in_worker(executable: &Path, job: &Job, limit: Duration) -> Result<Output, ImportError> {
    let input = serde_json::to_vec(&(job, limit.as_millis() as u64))
        .map_err(|e| ImportError::Encoding(e.to_string()))?;
    let mut command = std::process::Command::new(executable);
    command
        .arg(FLAG)
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
                    rlim_cur: MEMORY as libc::rlim_t,
                    rlim_max: MEMORY as libc::rlim_t,
                };
                if libc::setrlimit(libc::RLIMIT_AS, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    let mut child = command.spawn().map_err(ImportError::WorkerStart)?;
    let mut stdout = child.stdout.take().ok_or(ImportError::WorkerOutput)?;
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        (&mut stdout)
            .take(MAX_OUTPUT + 1)
            .read_to_end(&mut output)
            .map(|_| output)
    });
    // The worker reads everything before it starts; a worker that dies
    // early only makes this write fail.
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&input);
    }
    drop(input);
    let deadline = Instant::now() + limit + GRACE;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() > deadline => break None,
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.into());
            }
        }
    };
    let Some(status) = status else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(ImportError::WorkerTimeout(limit));
    };
    let output = reader
        .join()
        .ok()
        .and_then(Result::ok)
        .filter(|o| o.len() as u64 <= MAX_OUTPUT);
    match output {
        Some(output) if status.success() => {
            serde_json::from_slice::<Result<Output, String>>(&output)
                .map_err(|_| ImportError::WorkerAborted)?
                .map_err(ImportError::Reported)
        }
        _ => Err(ImportError::WorkerKilled),
    }
}
