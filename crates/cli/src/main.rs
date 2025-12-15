use std::path::PathBuf;

use dupdup_core::db::SqliteScanStore;
use dupdup_core::models::ScanRootKind;
use dupdup_core::scan::{scan_to_sqlite, ScanConfig};

fn main() {
    if let Err(err) = real_main() {
        eprintln!("error: {err}");
        std::process::exit(2);
    }
}

fn real_main() -> dupdup_core::Result<()> {
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
                    _ => return Err(dupdup_core::Error::InvalidArgument(format!("unknown arg: {arg}"))),
                }
            }

            let root = root.ok_or_else(|| {
                dupdup_core::Error::InvalidArgument("missing --root <path>".to_string())
            })?;
            let db = db.ok_or_else(|| dupdup_core::Error::InvalidArgument("missing --db <path>".to_string()))?;

            let store = SqliteScanStore::open(&db)?;
            let cfg = ScanConfig {
                root,
                root_kind,
                hash_files: true,
            };
            let res = scan_to_sqlite(&cfg, &store)?;

            println!("scan_id: {}", res.scan_id);
            println!(
                "files_seen: {}, hashed: {}, skipped: {}",
                res.stats.files_seen, res.stats.files_hashed, res.stats.files_skipped
            );
            Ok(())
        }
        _ => Err(dupdup_core::Error::InvalidArgument(format!(
            "unknown command: {cmd}"
        ))),
    }
}

fn print_help() {
    println!(
        r#"dupdup-cli

USAGE:
  dupdup-cli scan --root <path> --db <sqlite_path> [--drive|--folder]

NOTES:
  - Scans are stored as standalone SQLite DBs (one per scan).
  - UI crates are present but stubbed; the CLI is the initial entrypoint.
"#
    );
}

