//! Where this crate's log lines go. The hosting app installs a sink once at
//! startup; until then lines are dropped.

use std::fmt::Arguments;
use std::sync::OnceLock;

type Sink = Box<dyn Fn(Arguments) + Send + Sync>;

static SINK: OnceLock<Sink> = OnceLock::new();

/// Sends every later log line to `sink`. Only the first call has an effect.
pub fn set_sink(sink: impl Fn(Arguments) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(sink));
}

pub fn write(args: Arguments) {
    if let Some(sink) = SINK.get() {
        sink(args);
    }
}

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::logger::write(format_args!($($arg)*))
    };
}
