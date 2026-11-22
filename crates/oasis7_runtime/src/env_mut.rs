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

#[cfg(test)]
pub(crate) fn test_env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}
