use crate::TrieNode;
use std::fs;
use std::os::unix::{fs::PermissionsExt, process::CommandExt};
use std::path::{Path, PathBuf};

pub const BUILTINS: &[&str] = &["exit", "echo", "type", "pwd", "cd", "complete", "jobs"];

pub fn resolve_path(pathenv: &str, command: &str) -> Option<PathBuf> {
    let rawpaths: Vec<&str> = pathenv.split(":").collect();
    for path in rawpaths {
        let path = Path::new(path).join(command);
        if let Ok(metadata) = fs::metadata(&path) {
            if metadata.is_file() && (metadata.permissions().mode() & 0o111) != 0 {
                return Some(path);
            }
        }
    }
    return None;
}

pub fn build_exec_db(pathenv: &str, root: &mut TrieNode) {
    let rawpaths: Vec<&str> = pathenv.split(":").collect();
    for path in rawpaths {
        if let Ok(files) = fs::read_dir(path) {
            for file in files {
                let file_name = file.unwrap().file_name().into_string().unwrap();
                let path = Path::new(path).join(&file_name);
                if let Ok(metadata) = fs::metadata(&path) {
                    if metadata.is_file() && (metadata.permissions().mode() & 0o111) != 0 {
                        root.insert(file_name);
                    }
                }
            }
        }
    }
    for cmd in BUILTINS {
        root.insert(cmd.to_string());
    }
}
