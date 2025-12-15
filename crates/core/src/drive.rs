use std::path::Path;

use crate::error::Result;
use crate::models::DriveMetadata;

pub fn probe_for_path(path: &Path) -> Result<DriveMetadata> {
    #[cfg(target_os = "linux")]
    {
        return linux::probe_for_path(path);
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = path;
        return Ok(DriveMetadata {
            id: None,
            label: None,
            fs_type: None,
        });
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::fs;
    use std::io;
    use std::path::{Path, PathBuf};

    use crate::error::Result;
    use crate::models::DriveMetadata;

    pub fn probe_for_path(path: &Path) -> Result<DriveMetadata> {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let Some(mount) = best_mount_for_path(&canonical)? else {
            return Ok(DriveMetadata {
                id: None,
                label: None,
                fs_type: None,
            });
        };

        let (uuid, label) = if let Some(dev) = mount.mount_source.as_deref() {
            if dev.starts_with("/dev/") {
                let dev_path = fs::canonicalize(dev).unwrap_or_else(|_| PathBuf::from(dev));
                (
                    find_disk_id(&dev_path, Path::new("/dev/disk/by-uuid")).ok(),
                    find_disk_id(&dev_path, Path::new("/dev/disk/by-label")).ok(),
                )
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        Ok(DriveMetadata {
            id: uuid,
            label,
            fs_type: mount.fs_type,
        })
    }

    #[derive(Debug, Clone)]
    struct MountInfo {
        mount_point: PathBuf,
        fs_type: Option<String>,
        mount_source: Option<String>,
    }

    fn best_mount_for_path(path: &Path) -> io::Result<Option<MountInfo>> {
        let mountinfo = fs::read_to_string("/proc/self/mountinfo")?;
        let mut best: Option<MountInfo> = None;

        for line in mountinfo.lines() {
            let Some(mi) = parse_mountinfo_line(line) else {
                continue;
            };

            if !path.starts_with(&mi.mount_point) {
                continue;
            }

            let replace = match &best {
                None => true,
                Some(cur) => mi.mount_point.as_os_str().len() > cur.mount_point.as_os_str().len(),
            };
            if replace {
                best = Some(mi);
            }
        }

        Ok(best)
    }

    fn parse_mountinfo_line(line: &str) -> Option<MountInfo> {
        let (left, right) = line.split_once(" - ")?;
        let left_fields: Vec<&str> = left.split_whitespace().collect();
        if left_fields.len() < 5 {
            return None;
        }

        let mount_point = unescape_mountinfo(left_fields[4]);
        let right_fields: Vec<&str> = right.split_whitespace().collect();
        let fs_type = right_fields.first().map(|s| s.to_string());
        let mount_source = right_fields.get(1).map(|s| s.to_string());

        Some(MountInfo {
            mount_point: PathBuf::from(mount_point),
            fs_type,
            mount_source,
        })
    }

    fn unescape_mountinfo(s: &str) -> String {
        s.replace(r"\040", " ")
            .replace(r"\011", "\t")
            .replace(r"\012", "\n")
            .replace(r"\134", r"\")
    }

    fn find_disk_id(dev: &Path, dir: &Path) -> io::Result<String> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let path = entry.path();
            let target = fs::canonicalize(&path).unwrap_or(path);
            if target == dev {
                return Ok(name);
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no matching device id found",
        ))
    }
}
