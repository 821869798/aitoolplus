use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=resources.rc");
        println!("cargo:rerun-if-changed=assets/app.ico");

        let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let res_path = out_dir.join("resources.res");
        let rc_path = Path::new("resources.rc");

        if let Some(rc_exe) = find_rc_exe() {
            let status = Command::new(&rc_exe)
                .arg(format!("/fo{}", res_path.display()))
                .arg(rc_path)
                .status();

            match status {
                Ok(s) if s.success() => {
                    println!("cargo:rustc-link-arg={}", res_path.display());
                }
                Ok(s) => {
                    println!(
                        "cargo:warning=rc.exe exited with non-zero code: {:?}",
                        s.code()
                    );
                }
                Err(e) => {
                    println!("cargo:warning=failed to execute rc.exe: {e}");
                }
            }
        }
    }
}

#[cfg(windows)]
fn find_rc_exe() -> Option<PathBuf> {
    // 1. Direct check in PATH
    if let Ok(output) = Command::new("where.exe").arg("rc.exe").output()
        && output.status.success()
    {
        let path_str = String::from_utf8_lossy(&output.stdout);
        if let Some(first_line) = path_str.lines().next() {
            let p = PathBuf::from(first_line.trim());
            if p.is_file() {
                return Some(p);
            }
        }
    }

    // 2. Standard Windows Kits paths
    let roots = [
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.26100.0\x64\rc.exe",
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.22621.0\x64\rc.exe",
        r"C:\Program Files (x86)\Windows Kits\10\bin\10.0.19041.0\x64\rc.exe",
    ];
    for r in roots {
        let p = PathBuf::from(r);
        if p.is_file() {
            return Some(p);
        }
    }

    // 3. Search under Windows Kits\10\bin
    let base = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = std::fs::read_dir(base) {
        for entry in entries.flatten() {
            let candidate = entry.path().join("x64").join("rc.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}
