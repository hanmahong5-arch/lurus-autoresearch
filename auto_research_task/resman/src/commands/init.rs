use std::path::Path;

pub fn cmd_init(path: &Path) {
    if let Err(e) = std::fs::create_dir_all(path) {
        eprintln!("Failed to create directory: {}", e);
        return;
    }
    if let Err(e) = std::fs::create_dir_all(path.join("runs")) {
        eprintln!("Failed to create runs directory: {}", e);
        return;
    }
    println!("Initialized resman data directory: {}", path.display());
}
