//! Logging: file + console, rotating by size (simple truncation).

use std::path::Path;

/// Initialize tracing to a file, falling back to console-only.
pub fn init(log_file: &Path) {
    if let Some(parent) = log_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Truncate if the log grows past 2 MB (simple rotation).
    if let Ok(md) = std::fs::metadata(log_file)
        && md.len() > 2 * 1024 * 1024
    {
        let _ = std::fs::remove_file(log_file);
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_file);

    match file {
        Ok(f) => {
            use tracing_subscriber::layer::SubscriberExt;
            use tracing_subscriber::util::SubscriberInitExt;

            let file_writer = std::sync::Arc::new(std::sync::Mutex::new(f));
            let stdout_layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(std::io::stdout);
            let file_layer = tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(move || FileWriter(file_writer.clone()));

            let filter = tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into());

            tracing_subscriber::registry()
                .with(filter)
                .with(stdout_layer)
                .with(file_layer)
                .init();
        }
        Err(_) => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();
        }
    }
}

/// A `MakeWriter` cloning an Arc<Mutex<File>> per write.
struct FileWriter(std::sync::Arc<std::sync::Mutex<std::fs::File>>);

impl std::io::Write for FileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self.0.lock() {
            Ok(mut f) => f.write(buf),
            Err(_) => Ok(buf.len()), // poisoned writer: drop the line
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self.0.lock() {
            Ok(mut f) => f.flush(),
            Err(_) => Ok(()),
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for FileWriter {
    type Writer = FileWriter;
    fn make_writer(&'a self) -> Self::Writer {
        FileWriter(self.0.clone())
    }
}
