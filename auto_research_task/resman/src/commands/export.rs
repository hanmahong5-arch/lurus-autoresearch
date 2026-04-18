use std::fs;
use std::path::Path;

use crate::error::Result;
use crate::store::load_all_runs;

pub fn cmd_export(data_dir: &Path, output: &Path) -> Result<()> {
    let runs = load_all_runs(data_dir)?;
    if runs.is_empty() {
        eprintln!("no experiments found.");
        return Ok(());
    }
    let json = serde_json::to_string_pretty(&runs)?;
    fs::write(output, json)?;
    println!("exported {} run(s) to {}", runs.len(), output.display());
    Ok(())
}
