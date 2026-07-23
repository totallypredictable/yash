use crate::TrieNode;
use crate::env::BUILTINS;
use crate::tokenizer::tokenize;
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process;

pub enum CompCandidate {
    Nothing,
    ExpandBuffer { suffix: String, append_space: bool },
    ListCandidates { candidates: Vec<String> },
}

pub struct CompletionEngine {
    pub root: TrieNode,
}

impl CompletionEngine {
    pub fn new(pathenv: &str) -> Self {
        CompletionEngine {
            root: Self::build_exec_db(&pathenv),
        }
    }

    fn build_exec_db(pathenv: &str) -> TrieNode {
        let mut root = TrieNode::new();
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
        root
    }

    pub fn find_matches(
        &self,
        buffer: &str,
        complete_db: &HashMap<String, Vec<String>>,
    ) -> CompCandidate {
        let args = tokenize(buffer);
        if args.is_empty() {
            return CompCandidate::Nothing;
        }
        let mut results;
        let completion_prefix: String;
        if args.len() == 1 && !buffer.ends_with(' ') {
            completion_prefix = String::from(buffer);
            results = self.root.search(buffer);
        } else {
            let full_path;
            if buffer.ends_with(' ') {
                full_path = Path::new(".").to_owned();
                completion_prefix = String::new();
            } else if args.last().unwrap().ends_with('/') {
                full_path = Path::new(".").join(args.last().unwrap()).to_owned();
                completion_prefix = String::new();
            } else {
                full_path = Path::new(".")
                    .join(args.last().unwrap())
                    .parent()
                    .unwrap_or(Path::new("."))
                    .to_owned();
                completion_prefix = Path::new(args.last().unwrap())
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();
            }

            let mut tmp = Vec::new();
            let mut env_vars: Vec<(String, String)> = Vec::new();

            if let Some(value) = complete_db.get(&args[0]) {
                env_vars.push(("COMP_LINE".to_string(), String::from(buffer)));
                env_vars.push(("COMP_POINT".to_string(), buffer.len().to_string()));
                let output = run_completer_script(Path::new(&value[0]), &args, env_vars.clone());
                let stdout_result = String::from_utf8(output.stdout).unwrap();
                let outputs = stdout_result.trim().split('\n');

                for output in outputs {
                    if output.starts_with(&completion_prefix) {
                        tmp.push(output.to_string());
                    }
                }
            } else {
                let files = search_dir(&full_path);

                for file in &files {
                    if file.starts_with(&completion_prefix) {
                        tmp.push(file.to_string())
                    }
                }
            }

            results = tmp;
        }

        results.sort();

        if results.is_empty() {
            CompCandidate::Nothing
        } else if results.len() == 1 {
            CompCandidate::ExpandBuffer {
                suffix: String::from(
                    results[0]
                        .strip_prefix(&completion_prefix)
                        .expect("prefix check should've happened"),
                ),
                append_space: !results[0].ends_with('/'),
            }
        } else {
            let lcp = lcp(&results);
            if lcp.len() > completion_prefix.len() {
                CompCandidate::ExpandBuffer {
                    suffix: String::from(
                        lcp.strip_prefix(&completion_prefix)
                            .expect("prefix check should've happened"),
                    ),
                    append_space: false,
                }
            } else {
                CompCandidate::ListCandidates {
                    candidates: results,
                }
            }
        }
    }
}

fn lcp(results: &[String]) -> String {
    let mut lcp = String::new();
    for (i, ch) in results[0].chars().enumerate() {
        if results[1..].iter().all(|r| r.chars().nth(i) == Some(ch)) {
            lcp.push(ch);
        } else {
            break;
        }
    }
    lcp
}

fn search_dir(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if dir.is_dir() {
        if let Ok(drc) = fs::read_dir(dir) {
            for entry in drc {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_file() {
                        files.push(entry.file_name().into_string().unwrap());
                    }
                    if path.is_dir() {
                        let mut s = entry.file_name().into_string().unwrap();
                        s.push_str("/");
                        files.push(s);
                    }
                }
            }
        }
    }

    files
}

fn run_completer_script(
    path: &Path,
    args: &Vec<String>,
    env_vars: Vec<(String, String)>,
) -> std::process::Output {
    let args_list: Vec<String>;
    if args.len() == 2 {
        args_list = vec![args[0].clone(), args[1].clone(), args[0].clone()];
    } else if args.len() == 3 {
        args_list = vec![args[0].clone(), args[2].clone(), args[1].clone()];
    } else {
        args_list = vec![String::from("")];
    }
    let cmd = process::Command::new(path)
        .envs(env_vars)
        .args(args_list)
        .stdout(process::Stdio::piped())
        .spawn()
        .expect("Failed to run the completer script");

    let output = cmd
        .wait_with_output()
        .expect("Failed to wait on the completer script");

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_lcp() {
        assert_eq!(
            lcp(&[
                "alex".to_string(),
                "alexandra".to_string(),
                "alexis".to_string()
            ]),
            "alex".to_string()
        );
    }
}
