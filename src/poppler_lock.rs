use std::sync::{Mutex, MutexGuard};

static POPPLER_ACCESS: Mutex<()> = Mutex::new(());

pub fn lock() -> Result<MutexGuard<'static, ()>, String> {
    POPPLER_ACCESS
        .lock()
        .map_err(|_| "Poppler access lock was poisoned".to_string())
}
