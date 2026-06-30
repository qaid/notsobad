use rusqlite::Connection;
use std::sync::Mutex;

/// Shared app state. One SQLite connection behind a Mutex — single-user, local,
/// low concurrency. ponytail: a connection pool buys nothing here; add one only
/// if a real throughput problem shows up.
pub struct AppState {
    pub db: Mutex<Connection>,
}
