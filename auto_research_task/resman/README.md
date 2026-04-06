# resman

CLI tool for managing research experiments from [karpathy/autoresearch](https://github.com/karpathy/autoresearch) workflows. Import TSV results, track progress, compare runs, and generate HTML reports — all from the terminal.

```
$ resman list
     #   val_bpb      mem_gb    commit   status  description
--------------------------------------------------------------------------------------------------------------
     1    0.997900       44.0    a1b2c3d     keep  baseline
     2    0.993200       44.2    b2c3d4e     keep  increase LR to 0.04
     3    0.990100       44.1    c3d4e5f     keep  sliding window tuning
```

## Features

- **Import** `results.tsv` files from autoresearch runs
- **Parse** training logs (`run.log`) to extract val_bpb, VRAM, MFU, and more
- **List** experiments with filtering, sorting, and regex search
- **Compare** best experiments across multiple runs
- **Generate** HTML reports with trend charts (SVG, no JS dependency)
- **Export** full data as JSON for external analysis
- **Statistics**: improvement rate, crash rate, mean/std per run

No database. No network. No dependencies beyond a single binary.

## Installation

### From source (requires Rust 1.85+)

```bash
cargo install --path .
```

### Prebuilt binary

```bash
# Download from Releases
chmod +x resman
mv resman /usr/local/bin/
```

## Quick Start

```bash
# Initialize data directory
resman init

# Import results from an autoresearch run
resman import results.tsv -t apr2

# View all kept experiments sorted by val_bpb
resman list

# View top 5
resman list --top 5

# Filter with regex
resman list --grep "LR|learning.rate"

# Compare all runs
resman compare

# Generate HTML report
resman report report.html

# Show statistics
resman stats
```

## Command Reference

| Command | Description |
|---------|-------------|
| `init [path]` | Initialize resman data directory |
| `import <tsv>` | Import results.tsv from autoresearch |
| `parse-log <pattern>` | Extract metrics from training logs (glob pattern) |
| `list [options]` | List experiments (filter, sort, top N) |
| `compare [tags...]` | Compare best experiments across runs |
| `stats` | Statistical summary (improvement, crash rate, etc.) |
| `report <output.html>` | Generate HTML report with SVG trend chart |
| `export <output.json>` | Export all data as JSON |

### `list` options

| Flag | Description |
|------|-------------|
| `-s, --status <status>` | Filter by status (keep/discard/crash) |
| `-S, --sort-by <field>` | Sort field: val_bpb, memory_gb, description |
| `-g, --grep <regex>` | Filter description by regex |
| `-t, --top <N>` | Show top N results |
| `--reverse` | Reverse sort order |

## Data Layout

```
~/.resman/
  runs/
    apr2.json          # Imported run data
    apr3.json
    ...
```

Each JSON file contains the full experiment log with commit hashes, metrics, and metadata. The format is stable — you can also read it directly or import into your own tools.

## Architecture

```
results.tsv ──┐
              ├─► resman import ──► ~/.resman/runs/*.json ──► resman report ──► report.html
run.log ──────┘                         │
                                        ├─► resman list
                                        ├─► resman stats
                                        └─► resman compare
```

- **Storage**: JSON files (one per run), no database required
- **Metrics**: val_bpb (bits/byte, lower is better), peak VRAM, training time
- **Report**: Self-contained HTML with inline SVG charts

## License

MIT
