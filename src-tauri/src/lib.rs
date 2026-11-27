//! Scriptorium Tauri library entry.
//!
//! Hosts the desktop app runtime, the shared Rust modules (book format types, ingest, the
//! book library), and the Tauri commands the webview calls.

#![warn(missing_docs)]

pub mod book_format;
mod commands;
pub mod ingest;
pub mod library;
mod ocr;

use tauri::{Manager, RunEvent, WindowEvent};

/// Loads the pdfium library from the bundle (`Contents/Frameworks` on macOS, the resource
/// directory elsewhere). A failure is logged and
/// nothing more: only PDF import reports it, so images and existing books keep working.
///
/// A debug build falls back to the directory `scripts/fetch_pdfium.sh` fills, for `tauri dev`.
fn bind_pdfium(app: &tauri::App) {
    let mut dirs = Vec::new();
    if let Ok(resources) = app.path().resource_dir() {
        // macOS ships it in Contents/Frameworks, so the bundler signs it with the app's
        // identity. A dylib signed by anyone else fails library validation.
        if cfg!(target_os = "macos") {
            if let Some(contents) = resources.parent() {
                dirs.push(contents.join("Frameworks"));
            }
        }
        dirs.push(resources);
    }
    if cfg!(debug_assertions) {
        dirs.push(std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/pdfium"
        )));
    }
    // Binding is once per process, so pick the first directory that holds the library.
    let Some(dir) = dirs
        .iter()
        .find(|d| pdfium_render::prelude::Pdfium::pdfium_platform_library_name_at_path(d).is_file())
        .or(dirs.first())
    else {
        eprintln!("PDF import is unavailable: the app has no resource directory");
        return;
    };
    if let Err(e) = ingest::pdf::bind(dir) {
        eprintln!("PDF import is unavailable: {e}");
    }
}

/// Starts the Tauri app and returns when the app exits.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            bind_pdfium(app);
            commands::setup(app)
        })
        // Flush in Rust: a JavaScript close handler would race the window teardown.
        .on_window_event(|window, event| {
            if matches!(event, WindowEvent::CloseRequested { .. }) {
                commands::flush_all(window.app_handle());
            }
        });
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::inspect_folder,
        commands::create_book,
        commands::open_book,
        commands::save_progress,
        commands::close_book,
        commands::last_book,
    ]);
    builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
                commands::flush_all(app);
            }
        });
}

/// Counting allocator for the OCR memory-budget tests.
///
/// It counts allocations on a thread that has armed it, and on a `jpeg-decoder` worker thread
/// when exactly one meter is armed. Every other thread runs normally. Budget tests share one
/// mutex, so their peaks never overlap.
#[cfg(test)]
#[global_allocator]
static TEST_ALLOCATOR: test_alloc::Counting = test_alloc::Counting;

/// The counting allocator and the budget guard used by `ocr::input` tests.
#[cfg(test)]
pub(crate) mod test_alloc {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
    use std::sync::{Mutex, MutexGuard};

    /// The test allocator. Counts only allocations made by a thread that armed it.
    pub(crate) struct Counting;

    /// One budget measurement. `live` is bytes allocated while armed and not yet freed,
    /// including frees that happen on another thread.
    struct Meter {
        live: AtomicIsize,
        peak: AtomicUsize,
        used: AtomicBool,
    }

    impl Meter {
        const fn new() -> Self {
            Self {
                live: AtomicIsize::new(0),
                peak: AtomicUsize::new(0),
                used: AtomicBool::new(false),
            }
        }
    }

    /// Two meters, because the two-book permit test arms two threads at once.
    static METERS: [Meter; 2] = [Meter::new(), Meter::new()];
    static COMBINED_NOW: AtomicIsize = AtomicIsize::new(0);
    static COMBINED_PEAK: AtomicUsize = AtomicUsize::new(0);
    static BUDGET_LOCK: Mutex<()> = Mutex::new(());

    /// The meter this thread armed, or `NONE`.
    const NONE: usize = usize::MAX;
    /// Name prefix of the threads `jpeg-decoder` 0.3 spawns for a component sample plane.
    ///
    /// `MpscWorker` names them `worker thread for component {n}` even with the rayon feature
    /// off. The plane is allocated on that thread, so the sole armed meter has to count it.
    pub(crate) const JPEG_WORKER_PREFIX: &str = "worker thread for component ";
    /// The prefix the operating system actually keeps. Linux stores 15 bytes, so the name
    /// above is truncated to `worker thread f`.
    const JPEG_WORKER_OS_PREFIX: &[u8] = b"worker thread f";
    std::thread_local! {
        static MY_METER: Cell<usize> = const { Cell::new(NONE) };
    }

    fn my_meter() -> usize {
        MY_METER.try_with(Cell::get).unwrap_or(NONE)
    }

    /// The one meter that is armed, when exactly one is.
    fn sole_active_meter() -> Option<usize> {
        let mut found = None;
        for (index, meter) in METERS.iter().enumerate() {
            if meter.used.load(Ordering::Relaxed) {
                if found.is_some() {
                    return None;
                }
                found = Some(index);
            }
        }
        found
    }

    /// True when this thread is one of `jpeg-decoder`'s component workers.
    ///
    /// The name is read from the operating system. `thread::current` panics once a worker's
    /// thread-locals are already destroyed, which is exactly when its last frees run.
    fn jpeg_worker_thread() -> bool {
        let mut name = [0u8; 64];
        if !read_os_thread_name(&mut name) {
            return false;
        }
        let end = name
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(name.len());
        name[..end].starts_with(JPEG_WORKER_OS_PREFIX)
    }

    /// Copy this thread's operating-system name into `buf`. The result is not necessarily
    /// NUL-terminated when the name fills the buffer.
    #[cfg(unix)]
    fn read_os_thread_name(buf: &mut [u8]) -> bool {
        // SAFETY: `buf` is a writable byte slice, and `pthread_getname_np` only writes into it.
        // It does not allocate, so calling it from the global allocator cannot recurse.
        let rc = unsafe {
            libc::pthread_getname_np(libc::pthread_self(), buf.as_mut_ptr().cast(), buf.len())
        };
        rc == 0
    }

    /// Copy this thread's operating-system name into `buf` as ASCII. Non-ASCII names do not
    /// match the worker prefix, so they are ignored.
    #[cfg(windows)]
    fn read_os_thread_name(buf: &mut [u8]) -> bool {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut core::ffi::c_void;
            fn GetThreadDescription(thread: *mut core::ffi::c_void, name: *mut *mut u16) -> i32;
            fn LocalFree(ptr: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        }
        // SAFETY: `GetThreadDescription` writes a `LocalAlloc` pointer to `wide` on success,
        // and `LocalFree` releases that pointer. Neither uses the Rust allocator.
        unsafe {
            let mut wide = core::ptr::null_mut();
            if GetThreadDescription(GetCurrentThread(), &mut wide) < 0 || wide.is_null() {
                return false;
            }
            let mut index = 0;
            while index < buf.len() {
                let unit = *wide.add(index);
                if unit == 0 || unit > 0x7F {
                    break;
                }
                buf[index] = unit as u8;
                index += 1;
            }
            LocalFree(wide.cast());
        }
        true
    }

    /// Platforms without an operating-system thread name never match a worker.
    #[cfg(not(any(unix, windows)))]
    fn read_os_thread_name(_buf: &mut [u8]) -> bool {
        false
    }

    /// The meter that should record this allocation or free.
    ///
    /// An armed thread uses its own meter. A `jpeg-decoder` worker uses the sole armed meter,
    /// because that worker allocates the sample plane and frees rows the armed thread built.
    /// Any other thread is ignored, including tests that run in parallel.
    fn meter_index() -> Option<usize> {
        let index = my_meter();
        if index != NONE {
            return Some(index);
        }
        let sole = sole_active_meter()?;
        jpeg_worker_thread().then_some(sole)
    }

    fn add(meter: &Meter, size: usize) -> usize {
        let live = meter.live.fetch_add(size as isize, Ordering::Relaxed) + size as isize;
        let live = live.max(0) as usize;
        meter.peak.fetch_max(live, Ordering::Relaxed);
        live
    }

    fn subtract(meter: &Meter, size: usize) {
        let _ = meter
            .live
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
                Some(live.saturating_sub(size as isize))
            });
    }

    fn combined_subtract(size: usize) {
        let _ = COMBINED_NOW.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |live| {
            Some(live.saturating_sub(size as isize))
        });
    }

    fn on_alloc(size: usize) {
        let Some(index) = meter_index() else {
            return;
        };
        add(&METERS[index], size);
        let now = COMBINED_NOW.fetch_add(size as isize, Ordering::Relaxed) + size as isize;
        COMBINED_PEAK.fetch_max(now.max(0) as usize, Ordering::Relaxed);
    }

    fn on_dealloc(size: usize) {
        let Some(index) = meter_index() else {
            return;
        };
        subtract(&METERS[index], size);
        combined_subtract(size);
    }

    // SAFETY: every method delegates to `System` and only adds bookkeeping around it.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let pointer = System.alloc(layout);
            if !pointer.is_null() {
                on_alloc(layout.size());
            }
            pointer
        }

        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            on_dealloc(layout.size());
            System.dealloc(pointer, layout);
        }
    }

    /// A held budget meter and, for tests that serialize, the shared budget lock.
    pub(crate) struct Budget {
        _lock: Option<MutexGuard<'static, ()>>,
        meter: usize,
    }

    impl Budget {
        /// Take the shared budget lock and start counting this thread's allocations.
        pub(crate) fn arm() -> Self {
            let lock = lock_budget();
            Self::claim(Some(lock))
        }

        /// Start counting this thread without taking the lock. For worker threads whose test
        /// already holds the lock.
        pub(crate) fn arm_unlocked() -> Self {
            Self::claim(None)
        }

        fn claim(lock: Option<MutexGuard<'static, ()>>) -> Self {
            let meter = METERS
                .iter()
                .position(|meter| {
                    meter
                        .used
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                })
                .expect("every budget meter is in use");
            METERS[meter].live.store(0, Ordering::SeqCst);
            METERS[meter].peak.store(0, Ordering::SeqCst);
            MY_METER.try_with(|cell| cell.set(meter)).ok();
            Self { _lock: lock, meter }
        }

        /// The largest number of bytes this measurement had live.
        pub(crate) fn peak(&self) -> usize {
            METERS[self.meter].peak.load(Ordering::SeqCst)
        }

        /// Fail with `what` when the peak exceeded `limit` bytes.
        pub(crate) fn assert_within(&self, limit: usize, what: &str) {
            let peak = self.peak();
            assert!(
                peak <= limit,
                "{what}: peak {peak} bytes is over the {limit}-byte budget"
            );
        }
    }

    impl Drop for Budget {
        fn drop(&mut self) {
            MY_METER.try_with(|cell| cell.set(NONE)).ok();
            METERS[self.meter].live.store(0, Ordering::SeqCst);
            METERS[self.meter].used.store(false, Ordering::SeqCst);
        }
    }

    /// Take the mutex the budget and heavy-fixture tests share.
    pub(crate) fn lock_budget() -> MutexGuard<'static, ()> {
        BUDGET_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Forget the combined peak across armed threads.
    pub(crate) fn reset_combined() {
        COMBINED_NOW.store(0, Ordering::Relaxed);
        COMBINED_PEAK.store(0, Ordering::Relaxed);
    }

    /// The largest number of armed bytes at once across every thread.
    pub(crate) fn combined_peak() -> usize {
        COMBINED_PEAK.load(Ordering::Relaxed)
    }
}
