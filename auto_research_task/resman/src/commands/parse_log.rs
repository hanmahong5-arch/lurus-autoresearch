use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use regex::Regex;
use crate::store::truncate;

pub fn cmd_parse_log(pattern: &str) {
    let paths: Vec<String> = glob::glob(pattern)
        .unwrap()
        .filter_map(|p| p.ok().and_then(|pb| pb.to_str().map(String::from)))
        .collect();

    if paths.is_empty() {
        eprintln!("No files matched: {}", pattern);
        return;
    }

    let val_re = Regex::new(r"^val_bpb:\s+([\d.]+)").unwrap();
    let peak_vram_re = Regex::new(r"^peak_vram_mb:\s+([\d.]+)").unwrap();
    let train_sec_re = Regex::new(r"^training_seconds:\s+([\d.]+)").unwrap();
    let mfu_re = Regex::new(r"^mfu_percent:\s+([\d.]+)").unwrap();
    let total_tok_re = Regex::new(r"^total_tokens_M:\s+([\d.]+)").unwrap();
    let num_steps_re = Regex::new(r"^num_steps:\s+(\d+)").unwrap();
    let depth_re = Regex::new(r"^depth:\s+(\d+)").unwrap();

    println!("{:<22} {:>10} {:>10} {:>10} {:>10} {:>8} {:>6} {:>6}",
        "file", "val_bpb", "gpu_mb", "seconds", "MFU%", "tok_M", "steps", "depth");
    println!("{}", "-".repeat(106));

    for path in &paths {
        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let reader = BufReader::new(file);

        let mut val_bpb = 0.0;
        let mut peak_vram = 0.0;
        let mut train_sec = 0.0;
        let mut mfu = 0.0;
        let mut total_tok = 0.0;
        let mut num_steps = 0;
        let mut depth = 0;

        for line in reader.lines().filter_map(|l| l.ok()) {
            if let Some(caps) = val_re.captures(&line) { val_bpb = caps[1].parse().unwrap_or(0.0); }
            if let Some(caps) = peak_vram_re.captures(&line) { peak_vram = caps[1].parse().unwrap_or(0.0); }
            if let Some(caps) = train_sec_re.captures(&line) { train_sec = caps[1].parse().unwrap_or(0.0); }
            if let Some(caps) = mfu_re.captures(&line) { mfu = caps[1].parse().unwrap_or(0.0); }
            if let Some(caps) = total_tok_re.captures(&line) { total_tok = caps[1].parse().unwrap_or(0.0); }
            if let Some(caps) = num_steps_re.captures(&line) { num_steps = caps[1].parse().unwrap_or(0); }
            if let Some(caps) = depth_re.captures(&line) { depth = caps[1].parse().unwrap_or(0); }
        }

        let fname = Path::new(path).file_name().and_then(|s| s.to_str()).unwrap_or(path);
        println!("{:<22} {:>10.6} {:>9.1}mb {:>9.1}s {:>9.2}% {:>7.1}M {:>5} {:>6}",
            truncate(fname, 22), val_bpb, peak_vram, train_sec, mfu, total_tok, num_steps, depth);
    }
}
