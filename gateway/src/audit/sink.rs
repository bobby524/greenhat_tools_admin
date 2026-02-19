//! Audit event sinks — where events go after construction.
//!
//! Two sinks ship by default:
//!
//! | Sink | Config | Description |
//! |------|--------|-------------|
//! | **Stdout JSONL** | always active | One JSON object per line to stdout |
//! | **File JSONL** | `AUDIT_LOG_FILE` env var | Append-only file (rotated externally) |
//!
//! Both sinks are wrapped behind the [`AuditSink`] trait so additional
//! backends (Postgres, S3, etc.) can be plugged in later.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use metrics::counter;

use super::event::AuditEvent;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// A destination for audit events.
pub trait AuditSink: Send + Sync {
    /// Emit a single audit event.  Implementations must not panic.
    fn emit(&self, event: &AuditEvent);
}

// ---------------------------------------------------------------------------
// Stdout JSONL sink
// ---------------------------------------------------------------------------

/// Writes one JSON line per event to **stdout**.
#[derive(Debug, Default)]
pub struct StdoutSink;

impl AuditSink for StdoutSink {
    fn emit(&self, event: &AuditEvent) {
        if let Ok(line) = serde_json::to_string(event) {
            // Best-effort — don't crash the gateway if stdout is broken.
            let _ = writeln!(std::io::stdout().lock(), "{line}");
        }
    }
}

// ---------------------------------------------------------------------------
// File JSONL sink
// ---------------------------------------------------------------------------

/// Appends one JSON line per event to a file at `path`.
///
/// The file is opened in append mode on first write and kept open.
/// External tooling (logrotate, etc.) is expected to handle rotation.
pub struct FileSink {
    path: PathBuf,
    writer: Mutex<Option<std::fs::File>>,
}

impl FileSink {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            writer: Mutex::new(None),
        }
    }

    /// Open (or re-open) the file in append mode.
    fn open_file(&self) -> std::io::Result<std::fs::File> {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
    }

    fn writer_guard_or_recover(&self) -> MutexGuard<'_, Option<std::fs::File>> {
        match self.writer.lock() {
            Ok(g) => g,
            Err(poisoned) => {
                counter!(
                    "lock_poison_recoveries_total",
                    "component" => "audit_file_sink",
                    "lock" => "writer"
                )
                .increment(1);
                tracing::error!(path = %self.path.display(), "audit file sink lock poisoned; recovering with inner state");
                poisoned.into_inner()
            }
        }
    }
}

impl AuditSink for FileSink {
    fn emit(&self, event: &AuditEvent) {
        let line = match serde_json::to_string(event) {
            Ok(l) => l,
            Err(_) => return,
        };

        let mut guard = self.writer_guard_or_recover();

        // Lazy-open on first write.
        if guard.is_none() {
            match self.open_file() {
                Ok(f) => *guard = Some(f),
                Err(e) => {
                    tracing::error!(path = %self.path.display(), error = %e, "audit: failed to open log file");
                    return;
                }
            }
        }

        if let Some(ref mut file) = *guard {
            if writeln!(file, "{line}").is_err() {
                // File may have been moved/deleted — try reopening once.
                if let Ok(mut f) = self.open_file() {
                    let _ = writeln!(f, "{line}");
                    *guard = Some(f);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Composite sink (fan-out)
// ---------------------------------------------------------------------------

/// Fans out events to multiple sinks.
pub struct CompositeSink {
    sinks: Vec<Arc<dyn AuditSink>>,
}

impl CompositeSink {
    pub fn new(sinks: Vec<Arc<dyn AuditSink>>) -> Self {
        Self { sinks }
    }
}

impl AuditSink for CompositeSink {
    fn emit(&self, event: &AuditEvent) {
        for sink in &self.sinks {
            sink.emit(event);
        }
    }
}

// ---------------------------------------------------------------------------
// Factory — build the sink pipeline from env vars
// ---------------------------------------------------------------------------

/// Build the default sink pipeline from environment configuration.
///
/// - Always includes [`StdoutSink`].
/// - Adds [`FileSink`] when `AUDIT_LOG_FILE` is set and non-empty.
pub fn build_sink_from_env() -> Arc<dyn AuditSink> {
    let mut sinks: Vec<Arc<dyn AuditSink>> = vec![Arc::new(StdoutSink)];

    if let Ok(path) = std::env::var("AUDIT_LOG_FILE") {
        let path = path.trim().to_owned();
        if !path.is_empty() {
            tracing::info!(path = %path, "audit: file sink enabled");
            sinks.push(Arc::new(FileSink::new(path)));
        }
    }

    if sinks.len() == 1 {
        // Only stdout — unwrap the single sink to avoid the composite wrapper.
        sinks.into_iter().next().unwrap()
    } else {
        Arc::new(CompositeSink::new(sinks))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    /// A test sink that captures events in a `Vec`.
    #[derive(Default, Clone)]
    pub struct CaptureSink {
        pub events: Arc<Mutex<Vec<String>>>,
    }

    impl AuditSink for CaptureSink {
        fn emit(&self, event: &AuditEvent) {
            if let Ok(line) = serde_json::to_string(event) {
                self.events.lock().unwrap().push(line);
            }
        }
    }

    #[test]
    fn capture_sink_records_events() {
        let sink = CaptureSink::default();
        let evt = AuditEvent::new("test.event", "r1", "127.0.0.1", None, json!({}));
        sink.emit(&evt);
        let events = sink.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].contains("test.event"));
    }

    #[test]
    fn composite_fans_out() {
        let a = Arc::new(CaptureSink::default());
        let b = Arc::new(CaptureSink::default());
        let composite = CompositeSink::new(vec![a.clone(), b.clone()]);

        let evt = AuditEvent::new("test.fanout", "r2", "10.0.0.1", None, json!({}));
        composite.emit(&evt);

        assert_eq!(a.events.lock().unwrap().len(), 1);
        assert_eq!(b.events.lock().unwrap().len(), 1);
    }

    #[test]
    fn file_sink_writes_to_tempfile() {
        let dir = std::env::temp_dir().join("audit_sink_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("audit.jsonl");
        let _ = std::fs::remove_file(&path);

        let sink = FileSink::new(&path);
        let evt = AuditEvent::new("test.file", "r3", "10.0.0.2", None, json!({"key": "val"}));
        sink.emit(&evt);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test.file"));
        assert!(content.contains("r3"));

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn file_sink_recovers_after_poisoned_lock() {
        use std::thread;

        let dir = std::env::temp_dir().join("audit_sink_poison_test");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("audit.jsonl");
        let _ = std::fs::remove_file(&path);

        let sink = Arc::new(FileSink::new(&path));
        let poisoned = sink.clone();

        let _ = thread::spawn(move || {
            let _guard = poisoned.writer.lock().unwrap();
            panic!("intentional poison");
        })
        .join();

        let evt = AuditEvent::new("test.file.poison", "r4", "10.0.0.3", None, json!({}));
        sink.emit(&evt);

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("test.file.poison"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
}
