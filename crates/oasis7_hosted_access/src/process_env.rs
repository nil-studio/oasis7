use std::ffi::OsStr;

/// Sets a process environment variable.
///
/// # Safety
///
/// Callers must ensure this mutation is not raced against any other
/// environment read or write in the process.
pub unsafe fn set_var<K, V>(key: K, value: V)
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    unsafe {
        std::env::set_var(key, value);
    }
}

/// Removes a process environment variable.
///
/// # Safety
///
/// Callers must ensure this mutation is not raced against any other
/// environment read or write in the process.
pub unsafe fn remove_var<K>(key: K)
where
    K: AsRef<OsStr>,
{
    unsafe {
        std::env::remove_var(key);
    }
}

/// Returns the process-wide mutex used to serialize test mutations of
/// process environment variables across modules and crates in the same binary.
pub fn process_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

/// Thread-reentrant guard over [`process_env_lock`].
///
/// Allows helper functions that each acquire the process env lock to nest on
/// the same thread without deadlocking while still serializing concurrent
/// threads.
pub struct ProcessEnvGuard {
    _guard: Option<std::sync::MutexGuard<'static, ()>>,
}

std::thread_local! {
    static PROCESS_ENV_LOCK_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl Drop for ProcessEnvGuard {
    fn drop(&mut self) {
        PROCESS_ENV_LOCK_DEPTH.with(|depth| {
            let current = depth.get();
            debug_assert!(current > 0);
            depth.set(current.saturating_sub(1));
        });
    }
}

/// Acquires [`process_env_lock`] reentrantly on the current thread.
pub fn lock_process_env() -> ProcessEnvGuard {
    let acquire_mutex = PROCESS_ENV_LOCK_DEPTH.with(|depth| depth.get() == 0);
    let guard = if acquire_mutex {
        Some(
            process_env_lock()
                .lock()
                .unwrap_or_else(|poison| poison.into_inner()),
        )
    } else {
        None
    };
    PROCESS_ENV_LOCK_DEPTH.with(|depth| depth.set(depth.get() + 1));
    ProcessEnvGuard { _guard: guard }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn lock_process_env_is_reentrant_on_same_thread_and_excludes_other_threads() {
        let outer = lock_process_env();
        let inner = lock_process_env();

        let (acquired_tx, acquired_rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _other = lock_process_env();
            acquired_tx.send(()).expect("notify acquired");
        });

        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "second thread must wait while outer guard is held"
        );

        drop(inner);
        assert!(
            acquired_rx.recv_timeout(Duration::from_millis(50)).is_err(),
            "second thread must still wait until outer guard is dropped"
        );

        drop(outer);
        acquired_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("second thread should acquire lock after outer guard drops");
        handle.join().expect("join thread");
    }
}
