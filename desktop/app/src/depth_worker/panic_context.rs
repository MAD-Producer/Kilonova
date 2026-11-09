//! Preserve the first panic cause before the logger's potentially long backtrace.
use std::{
    fs::OpenOptions,
    io::{self, Write},
    path::PathBuf,
};

fn bounded(value: &str, limit: usize) -> String {
    value
        .chars()
        .take(limit)
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub fn install(path: PathBuf) {
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("non-string panic payload");
        let cause = bounded(payload, 1536);
        let thread = std::thread::current();
        let location = info
            .location()
            .map(|v| format!("{}:{}:{}", bounded(v.file(), 512), v.line(), v.column()));
        let short_location = location
            .as_deref()
            .unwrap_or("unavailable")
            .chars()
            .rev()
            .take(160)
            .collect::<String>();
        let short_location = short_location.chars().rev().collect::<String>();
        let context = crate::engine::current_command().map(|(request, operation, elapsed)|
            serde_json::json!({"request":request,"operation":operation,"elapsed_ms":elapsed.as_millis()}));
        let receipt = serde_json::json!({"schema":1,"status":"native_panic_original_cause_retained",
            "pid":std::process::id(),"cause":cause,"location":location,
            "thread":bounded(thread.name().unwrap_or("unnamed"),128),"in_flight":context});
        // No logger or business mutex is acquired here. create_new preserves
        // the first cause if several worker threads fail while unwinding.
        let saved = (|| -> io::Result<()> {
            let bytes = serde_json::to_vec(&receipt).map_err(io::Error::other)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?;
            file.write_all(&bytes)?;
            file.flush()
        })();
        if let Err(error) = saved {
            if error.kind() != io::ErrorKind::AlreadyExists || !path.is_file() {
                let _ = writeln!(
                    io::stderr(),
                    "ERROR motion_depth_wgsl: panic receipt unavailable cause={error}"
                );
            }
        }
        // The shared hook force-captures the stack with try_lock, then calls
        // its original hook. Keep normal fatal unwind behavior unchanged.
        original(info);
        // capture() deliberately retains only the final 512 stderr characters.
        // A short summary after the original hooks survives even if the receipt
        // could not be written. It contains no backtrace or unbounded payload.
        let _ = writeln!(
            io::stderr(),
            "ERROR motion_depth_wgsl: panic cause={} at={short_location}",
            bounded(payload, 256)
        );
    }));
}
