//! Shared terminal presentation helpers for Vinpst binaries.

mod table;

use serde::Serialize;

pub use table::{print_rows, print_table};

/// Pretty-print one serializable value as JSON on stdout.
pub fn print_json(value: &impl Serialize) -> serde_json::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
