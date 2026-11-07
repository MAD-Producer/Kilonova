// Shared file/stderr sink; test declarations remain in logging.rs.
use std::{
    backtrace::Backtrace,
    cell::Cell,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct FileLogger {
    writer: Mutex<BufWriter<File>>,
    level: log::LevelFilter,
}

static LOGGER: OnceLock<FileLogger> = OnceLock::new();
thread_local! {
    static IN_PANIC_HOOK: Cell<bool> = const { Cell::new(false) };
}

impl FileLogger {
    fn lock_writer(&self) -> std::sync::MutexGuard<'_, BufWriter<File>> {
        match self.writer.lock() {
            Ok(writer) => writer,
            Err(poisoned) => {
                let mut writer = poisoned.into_inner();
                // A formatter can panic after writing part of a record.
                // Separate the next record and keep this file sink usable.
                let _ = writer.write_all(b"\n");
                self.writer.clear_poison();
                writer
            }
        }
    }
    fn write(&self, record: &log::Record<'_>, writer: &mut impl Write) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let thread = std::thread::current();
        let _ = writeln!(
            writer,
            "{}.{:03} {} [{}] {}: {}",
            timestamp.as_secs(),
            timestamp.subsec_millis(),
            record.level(),
            thread.name().unwrap_or("unnamed"),
            record.target(),
            record.args()
        );
        if record.level() <= log::Level::Warn {
            let _ = writer.flush();
        }
    }
}
impl log::Log for FileLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= self.level
            // Wgpu emits per-frame descriptors at Debug. Keep ordinary debug
            // sessions focused on Motion; Trace explicitly includes dependencies.
            && (self.level != log::LevelFilter::Debug
                || metadata.level() <= log::Level::Info
                || metadata.target().starts_with("motion_"))
    }
    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        if IN_PANIC_HOOK.get() {
            // The previous panic hook already preserves console stderr output.
            // The panic may have originated while this very thread held writer.
            match self.writer.try_lock() {
                Ok(mut writer) => self.write(record, &mut *writer),
                Err(_) => {
                    let _ = writeln!(io::stderr(), "{}: {}", record.level(), record.args());
                }
            }
        } else {
            let mut writer = self.lock_writer();
            self.write(record, &mut *writer);
        }
        if record.level() == log::Level::Error && !IN_PANIC_HOOK.get() {
            // Flush the file and release its mutex before console IO: an MCP
            // client may pipe stderr without draining it. Keep the cause on disk.
            // stdout remains the command-list / JSON-RPC transport.
            let _ = writeln!(io::stderr(), "ERROR {}: {}", record.target(), record.args());
        }
    }
    fn flush(&self) {
        let _ = self.lock_writer().flush();
    }
}

/// Flush buffered info/debug records when main returns, including early CLI exits.
pub struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        flush();
    }
}
pub fn flush() {
    log::logger().flush();
}

pub fn init(dir: PathBuf) -> Result<Guard, Box<dyn std::error::Error>> {
    fs::create_dir_all(&dir)?;
    let path = dir.join("motion-studio.log");
    if path.exists() {
        let previous = dir.join("motion-studio.log.1");
        match fs::remove_file(&previous) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::rename(&path, previous)?;
    }
    let level = std::env::var("MOTION_LOG")
        .ok()
        .and_then(|level| level.parse().ok())
        .unwrap_or(log::LevelFilter::Info);
    let logger = FileLogger {
        writer: Mutex::new(BufWriter::new(
            OpenOptions::new().create(true).append(true).open(path)?,
        )),
        level,
    };
    LOGGER
        .set(logger)
        .map_err(|_| io::Error::other("logging already initialized"))?;
    let logger = LOGGER
        .get()
        .ok_or_else(|| io::Error::other("logger unavailable"))?;
    log::set_logger(logger).map_err(|_| io::Error::other("process logger already installed"))?;
    log::set_max_level(level);
    let original = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        IN_PANIC_HOOK.set(true);
        log::error!(
            "panic command={:?}: {info}\n{}",
            crate::engine::current_command(),
            Backtrace::force_capture()
        );
        IN_PANIC_HOOK.set(false);
        original(info);
    }));
    std::thread::Builder::new()
        .name("motion-log-flush".into())
        .spawn(|| loop {
            std::thread::sleep(Duration::from_secs(1));
            flush();
        })?;
    log::info!("logging initialized level={level}");
    Ok(Guard)
}
