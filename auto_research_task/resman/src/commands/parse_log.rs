use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use regex::Regex;

use crate::error::Result;
use crate::store::truncate;

pub fn cmd_parse_log(pattern: &str) -> Result<()> {
    let paths: Vec<String> = glob::glob(pattern)?
        .filter_map(|p| p.ok().and_then(|pb| pb.to_str().map(String::from)))
        .collect();

    if paths.is_empty() {
        eprintln!("no files matched: {pattern}");
        return Ok(());
    }

    let specs: &[(&str, Regex)] = &[
        ("val_bpb", Regex::new(r"^val_bpb:\s+([\d.]+)")?),
        ("peak_vram_mb", Regex::new(r"^peak_vram_mb:\s+([\d.]+)")?),
        (
            "training_seconds",
            Regex::new(r"^training_seconds:\s+([\d.]+)")?,
        ),
        ("mfu_percent", Regex::new(r"^mfu_percent:\s+([\d.]+)")?),
        (
            "total_tokens_M",
            Regex::new(r"^total_tokens_M:\s+([\d.]+)")?,
        ),
        ("num_steps", Regex::new(r"^num_steps:\s+(\d+)")?),
        ("depth", Regex::new(r"^depth:\s+(\d+)")?),
    ];

    println!(
        "{:<22} {:>10} {:>10} {:>10} {:>8} {:>8} {:>6} {:>6}",
        "file", "val_bpb", "gpu_mb", "seconds", "mfu%", "tok_M", "steps", "depth"
    );
    println!("{}", "-".repeat(96));

    for path in &paths {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);
        let mut values: [Option<f64>; 7] = [None; 7];

        for line in reader.lines().map_while(|l| l.ok()) {
            for (i, (_, re)) in specs.iter().enumerate() {
                if let Some(caps) = re.captures(&line) {
                    values[i] = caps[1].parse().ok();
                }
            }
        }

        let fname = Path::new(path)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(path);
        let g = |i: usize| values[i].unwrap_or(0.0);
        println!(
            "{:<22} {:>10.6} {:>10.1} {:>10.1} {:>7.2}% {:>7.1}M {:>6} {:>6}",
            truncate(fname, 22),
            g(0),
            g(1),
            g(2),
            g(3),
            g(4),
            g(5) as i64,
            g(6) as i64
        );
    }
    Ok(())
}
