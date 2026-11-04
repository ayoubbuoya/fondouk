//! Fondouk backend as a library. `main.rs` is a thin starter that wires these
//! modules together; keeping the logic here lets tests (and later the `tests/`
//! folder) use every module through its public API.

pub mod agent;
pub mod api;
pub mod backtest;
pub mod config;
pub mod db;
pub mod indexer;
pub mod sim;
