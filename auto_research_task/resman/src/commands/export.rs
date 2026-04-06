use std::path::Path;
use crate::store::load_all_runs;

pub fn cmd_export(data_dir: &Path, output: &Path) {
    let all = load_all_runs(data_dir);
    if all.is_empty() {
        eprintln!("No experiments found.");
        return;
    }

    let json = serde_json::to_string_pretty(&all).unwrap();
    if let Err(e) = std::fs::write(output, json) {
        eprintln!("Failed to write: {}", e);
    } else {
        println!("Exported {} runs to {}", all.len(), output.display());
    }
}
