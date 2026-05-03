//! CLI subcommand implementations. Each module is a thin glue layer
//! over the corresponding lib crate(s) — the constitution's "thin
//! CLI, fat libs" rule.

pub mod migrate;
pub mod scan;
pub mod schemas;
pub mod util;
