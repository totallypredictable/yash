use std::env;
use std::fs;
#[allow(unused_imports)]
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn resolve_path(pathenv: &str, command: &str) -> Option<PathBuf> {
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

fn main() {
    let pathenv = env::var("PATH").unwrap();

    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut command: String = String::new();

        io::stdin().read_line(&mut command).unwrap();

        let words: Vec<&str> = command.trim().split_whitespace().collect();

        let builtins = ["exit", "echo", "type"];

        match words[0].trim() {
            "exit" => break,
            "echo" => println!("{}", words[1..].join(" ")),
            "type" => {
                if builtins.contains(&words[1]) {
                    println!("{} is a shell builtin", words[1]);
                } else if resolve_path(&pathenv, words[1]) != None {
                    println!(
                        "{} is {:?}",
                        words[1],
                        resolve_path(&pathenv, words[1]).unwrap().to_str() // TODO unnecessary double call. Call the function one and match against its return?
                    );
                } else {
                    println!("{}: not found", words[1]);
                }
            }
            other => println!("{}: command not found", other.trim()),
        }
    }
}
