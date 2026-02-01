use std::path::PathBuf;

use dupdupninja_core::db::SqliteScanStore;
use dupdupninja_core::models::ScanRootKind;
use dupdupninja_core::scan::{scan_to_sqlite, ScanConfig};

mod web;

fn main() {
    if let Err(err) = real_main() {
        eprintln!("error: {err}");
        std::process::exit(2);
    }
}

fn real_main() -> dupdupninja_core::Result<()> {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        print_help();
        return Ok(());
    };

    match cmd.as_str() {
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        "scan" => {
            let mut root: Option<PathBuf> = None;
            let mut db: Option<PathBuf> = None;
            let mut root_kind = ScanRootKind::Folder;

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--root" => root = args.next().map(PathBuf::from),
                    "--db" => db = args.next().map(PathBuf::from),
                    "--drive" => root_kind = ScanRootKind::Drive,
                    "--folder" => root_kind = ScanRootKind::Folder,
                    _ => {
                        return Err(dupdupninja_core::Error::InvalidArgument(format!(
                            "unknown arg: {arg}"
                        )));
                    }
                }
            }

            let root = root.ok_or_else(|| {
                dupdupninja_core::Error::InvalidArgument("missing --root <path>".to_string())
            })?;
            let db = db.ok_or_else(|| {
                dupdupninja_core::Error::InvalidArgument("missing --db <path>".to_string())
            })?;

            let store = SqliteScanStore::open(&db)?;
            let cfg = ScanConfig {
                root,
                root_kind,
                hash_files: true,
                perceptual_hashes: true,
                capture_snapshots: true,
                snapshots_per_video: 3,
                snapshot_max_dim: 1024,
            };
            let res = scan_to_sqlite(&cfg, &store)?;

            println!(
                "files_seen: {}, hashed: {}, skipped: {}",
                res.stats.files_seen, res.stats.files_hashed, res.stats.files_skipped
            );
            Ok(())
        }
        "matches" => {
            let mut db: Option<PathBuf> = None;
            let mut max_files: usize = 500;
            let mut ahash_thresh: u32 = 10;
            let mut dhash_thresh: u32 = 10;
            let mut phash_thresh: u32 = 8;

            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--db" => db = args.next().map(PathBuf::from),
                    "--max-files" => {
                        if let Some(val) = args.next() {
                            max_files = val.parse().map_err(|_| {
                                dupdupninja_core::Error::InvalidArgument(format!(
                                    "invalid --max-files value: {val}"
                                ))
                            })?;
                        }
                    }
                    "--ahash" => {
                        if let Some(val) = args.next() {
                            ahash_thresh = val.parse().map_err(|_| {
                                dupdupninja_core::Error::InvalidArgument(format!(
                                    "invalid --ahash value: {val}"
                                ))
                            })?;
                        }
                    }
                    "--dhash" => {
                        if let Some(val) = args.next() {
                            dhash_thresh = val.parse().map_err(|_| {
                                dupdupninja_core::Error::InvalidArgument(format!(
                                    "invalid --dhash value: {val}"
                                ))
                            })?;
                        }
                    }
                    "--phash" => {
                        if let Some(val) = args.next() {
                            phash_thresh = val.parse().map_err(|_| {
                                dupdupninja_core::Error::InvalidArgument(format!(
                                    "invalid --phash value: {val}"
                                ))
                            })?;
                        }
                    }
                    _ => {
                        return Err(dupdupninja_core::Error::InvalidArgument(format!(
                            "unknown arg: {arg}"
                        )));
                    }
                }
            }

            let db = db.ok_or_else(|| {
                dupdupninja_core::Error::InvalidArgument("missing --db <path>".to_string())
            })?;
            let store = SqliteScanStore::open(&db)?;
            let files = store.list_files_with_hashes(max_files, 0)?;

            if files.len() == max_files {
                eprintln!("warning: reached --max-files limit; results may be incomplete");
            }

            let mut uf = UnionFind::new(files.len());
            for i in 0..files.len() {
                for j in (i + 1)..files.len() {
                    let a = &files[i];
                    let b = &files[j];
                    if match_similarity(a, b, ahash_thresh, dhash_thresh, phash_thresh).is_some()
                    {
                        uf.union(i, j);
                    }
                }
            }

            let mut groups: std::collections::HashMap<usize, Vec<usize>> =
                std::collections::HashMap::new();
            for idx in 0..files.len() {
                let root = uf.find(idx);
                groups.entry(root).or_default().push(idx);
            }

            let mut group_list: Vec<Vec<usize>> = groups
                .into_values()
                .filter(|members| members.len() > 1)
                .collect();
            group_list.sort_by(|a, b| b.len().cmp(&a.len()));

            if group_list.is_empty() {
                println!("no similar matches found");
            } else {
                println!(
                    "similarity thresholds: ahash<= {}, dhash<= {}, phash<= {}",
                    ahash_thresh, dhash_thresh, phash_thresh
                );
                for (idx, members) in group_list.iter().enumerate() {
                    println!("Group {} ({} files):", idx + 1, members.len());
                    for member_idx in members {
                        println!("  {}", files[*member_idx].path.display());
                    }
                }
            }

            Ok(())
        }
        "web" => {
            let mut port: u16 = 4455;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--port" => {
                        if let Some(val) = args.next() {
                            port = val.parse().map_err(|_| {
                                dupdupninja_core::Error::InvalidArgument(format!(
                                    "invalid --port value: {val}"
                                ))
                            })?;
                        }
                    }
                    _ => {
                        return Err(dupdupninja_core::Error::InvalidArgument(format!(
                            "unknown arg: {arg}"
                        )));
                    }
                }
            }
            web::run_web_server(port)?;
            Ok(())
        }
        _ => Err(dupdupninja_core::Error::InvalidArgument(format!(
            "unknown command: {cmd}"
        ))),
    }
}

fn print_help() {
    println!(
        r#"dupdupninja

USAGE:
  dupdupninja scan --root <path> --db <sqlite_path> [--drive|--folder]
  dupdupninja matches --db <sqlite_path> [--max-files <n>] [--ahash <n>] [--dhash <n>] [--phash <n>]
  dupdupninja web [--port <port>]

NOTES:
  - Filesets are stored as standalone SQLite DBs (one per scan).
  - UI crates are present but stubbed; the CLI is the initial entrypoint.
  - Web UI listens on http://127.0.0.1:4455 by default.
"#
    );
}

fn match_similarity(
    a: &dupdupninja_core::models::FileListRow,
    b: &dupdupninja_core::models::FileListRow,
    ahash_thresh: u32,
    dhash_thresh: u32,
    phash_thresh: u32,
) -> Option<(&'static str, u32)> {
    if let (Some(a_hash), Some(b_hash)) = (a.phash, b.phash) {
        let dist = hamming_distance(a_hash, b_hash);
        if dist <= phash_thresh {
            return Some(("phash", dist));
        }
    }
    if let (Some(a_hash), Some(b_hash)) = (a.dhash, b.dhash) {
        let dist = hamming_distance(a_hash, b_hash);
        if dist <= dhash_thresh {
            return Some(("dhash", dist));
        }
    }
    if let (Some(a_hash), Some(b_hash)) = (a.ahash, b.ahash) {
        let dist = hamming_distance(a_hash, b_hash);
        if dist <= ahash_thresh {
            return Some(("ahash", dist));
        }
    }
    None
}

fn hamming_distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

struct UnionFind {
    parent: Vec<usize>,
    size: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            size: vec![1; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            let root = self.find(self.parent[x]);
            self.parent[x] = root;
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let mut root_a = self.find(a);
        let mut root_b = self.find(b);
        if root_a == root_b {
            return;
        }
        if self.size[root_a] < self.size[root_b] {
            std::mem::swap(&mut root_a, &mut root_b);
        }
        self.parent[root_b] = root_a;
        self.size[root_a] += self.size[root_b];
    }
}
