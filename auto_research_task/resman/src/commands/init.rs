use std::path::Path;

use crate::error::Result;
use crate::store::ensure_initialized;

pub fn cmd_init(path: &Path) -> Result<()> {
    ensure_initialized(path)?;
    println!("initialized resman data directory: {}", path.display());
    println!("  runs/    — per-run experiment logs (one JSON each)");
    println!();
    println!("next: `resman import results.tsv` or `resman add --tag <t> ...`");
    Ok(())
}
