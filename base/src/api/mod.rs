mod handler_index;
mod handler_info;
mod handler_metrics;
mod handler_status;

// Re-export handlers.
pub use self::handler_index::index;
pub use self::handler_info::InfoHandler;
pub use self::handler_metrics::MetricsHandler;
pub use self::handler_status::StatusHandler;
