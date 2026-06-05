use std::collections;
use std::env;
use std::fs;
#[allow(unused_imports)]
use std::io::{self, Write};
use std::ops::ControlFlow;
use std::os::unix::{fs::PermissionsExt, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process;

const BUILTINS: &[&str] = &["exit", "echo", "type", "pwd", "cd"];

fn env_ops<F, R>(action: F, cmd: &str, path: &Path) -> Option<R>
where
    F: FnOnce() -> io::Result<R>,
{
    match action() {
        Ok(res) => Some(res),
        Err(_) => {
            eprintln!("{}: {}: No such file or directory", cmd, path.display());
            None
        }
    }
}

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

fn prompt() {
    print!("$ ");
    io::stdout().flush().unwrap();
}

fn read_input() -> String {
    let mut command: String = String::new();
    io::stdin().read_line(&mut command).unwrap();
    command
}

enum Backslash {
    Yes,
    No,
}

enum TokenizerState {
    Normal(Backslash),
    InSingleQuote,
    InDoubleQuote(Backslash),
}

fn tokenize(command: &str) -> Vec<String> {
    let mut state = TokenizerState::Normal(Backslash::No);
    let mut args: Vec<String> = Vec::new();
    let mut buffer = String::new();

    for ch in command.chars() {
        state = match state {
            TokenizerState::Normal(Backslash::No) => match ch {
                ' ' => {
                    if !buffer.is_empty() {
                        args.push(std::mem::take(&mut buffer));
                    }
                    TokenizerState::Normal(Backslash::No)
                }
                '\\' => TokenizerState::Normal(Backslash::Yes),
                '"' => TokenizerState::InDoubleQuote(Backslash::No),
                '\'' => TokenizerState::InSingleQuote,
                '~' => {
                    if let Ok(home) = env::var("HOME") {
                        buffer.push_str(&home);
                    }
                    TokenizerState::Normal(Backslash::No)
                }
                _ => {
                    buffer.push(ch);
                    TokenizerState::Normal(Backslash::No)
                }
            },

            TokenizerState::Normal(Backslash::Yes) => {
                buffer.push(ch);
                TokenizerState::Normal(Backslash::No)
            }

            TokenizerState::InDoubleQuote(Backslash::No) => match ch {
                '\\' => TokenizerState::InDoubleQuote(Backslash::Yes),
                '"' => TokenizerState::Normal(Backslash::No),
                _ => {
                    buffer.push(ch);
                    TokenizerState::InDoubleQuote(Backslash::No)
                }
            },

            TokenizerState::InDoubleQuote(Backslash::Yes) => match ch {
                _ if (['"', '\\', '$', '`', '\n']).contains(&ch) => {
                    buffer.push(ch);
                    TokenizerState::InDoubleQuote(Backslash::No)
                }
                _ => {
                    buffer.push('\\');
                    buffer.push(ch);
                    TokenizerState::InDoubleQuote(Backslash::No)
                }
            },

            TokenizerState::InSingleQuote => match ch {
                '\'' => TokenizerState::Normal(Backslash::No),
                _ => {
                    buffer.push(ch);
                    TokenizerState::InSingleQuote
                }
            },
        };
        // match (&state, ch) {
        //     (TokenizerState::Backslash, _) => {
        //         buffer.push(ch);
        //         state = TokenizerState::Normal;
        //     }
        //     (TokenizerState::Normal, ' ') => {
        //         if !buffer.is_empty() {
        //             args.push(std::mem::take(&mut buffer));
        //         }
        //     }
        //     (TokenizerState::Normal, '"') => {
        //         state = TokenizerState::InDoubleQuote;
        //     }
        //     (TokenizerState::Normal, '\'') => state = TokenizerState::InSingleQuote,
        //     (TokenizerState::Normal, '~') => {
        //         if let Ok(home) = env::var("HOME") {
        //             buffer.push_str(&home);
        //         };
        //     }
        //     (TokenizerState::Normal, '\\') => {
        //         state = TokenizerState::Backslash;
        //     }
        //     (TokenizerState::Normal, _) => {
        //         buffer.push(ch);
        //     }
        //     (TokenizerState::InDoubleQuote, '"') => state = TokenizerState::Normal,
        //     (TokenizerState::InDoubleQuote, _) => buffer.push(ch),
        //     (TokenizerState::InSingleQuote, '\'') => state = TokenizerState::Normal,
        //     (TokenizerState::InSingleQuote, _) => buffer.push(ch),
        // }
    }

    if !buffer.is_empty() {
        args.push(buffer);
    }
    args
}

enum Command {
    Exit,
    Echo(Vec<String>),
    Type(Vec<String>),
    Pwd,
    Cd(Vec<String>),
    External(String, Vec<String>),
}

struct ParsedCommand {
    cmd: Command,
}

impl ParsedCommand {
    fn new(mut words: Vec<String>) -> Option<Self> {
        if words.is_empty() {
            return None;
        }
        let mut words = collections::VecDeque::from(words);
        let first_token: String = words.pop_front().unwrap();
        let remaining_tokens = words.into_iter().collect();
        let command_type = match first_token.as_str() {
            "exit" => Command::Exit,
            "echo" => Command::Echo(remaining_tokens),
            "type" => Command::Type(remaining_tokens),
            "pwd" => Command::Pwd,
            "cd" => Command::Cd(remaining_tokens),
            _ => Command::External(first_token, remaining_tokens),
        };
        Some(ParsedCommand { cmd: command_type })
    }
}

fn dispatch_command(pathenv: &str, parsed_command: ParsedCommand) -> ControlFlow<()> {
    match parsed_command.cmd {
        Command::Exit => ControlFlow::Break(()),
        Command::Echo(args) => {
            println!("{}", args.join(" "));
            ControlFlow::Continue(())
        }
        Command::Type(cmds) => {
            for cmd in cmds {
                if BUILTINS.contains(&&cmd.as_str()) {
                    println!("{} is a shell builtin", cmd);
                } else if let Some(path) = resolve_path(pathenv, cmd.as_str()) {
                    println!("{} is {}", cmd, path.display(),);
                } else {
                    println!("{}: not found", cmd);
                }
            }
            ControlFlow::Continue(())
        }
        Command::Pwd => {
            if let Ok(path) = env::current_dir() {
                println!("{}", path.display())
            }
            ControlFlow::Continue(())
        }
        Command::Cd(path) => {
            let target_path: Option<PathBuf> = match path.get(0) {
                Some(p) => Some(PathBuf::from(p)),
                None => match env::var("HOME") {
                    Ok(h) => Some(PathBuf::from(h)),
                    Err(_) => {
                        eprintln!("cd: HOME not set");
                        None
                    }
                },
            };

            if let Some(path) = target_path {
                env_ops(|| env::set_current_dir(&path), "cd", &path);
            }

            ControlFlow::Continue(())
        }
        Command::External(bin, args) => {
            if let Some(path) = resolve_path(pathenv, bin.as_str()) {
                run_program(&path, bin.as_str(), &args);
                ControlFlow::Continue(())
            } else {
                println!("{}: command not found", bin);
                ControlFlow::Continue(())
            }
        }
    }
}

fn run_program(path: &Path, bin: &str, args: &[String]) {
    let mut cmd = process::Command::new(path);
    cmd.arg0(bin);
    cmd.args(args);
    match cmd.spawn() {
        Ok(mut handle) => match handle.wait() {
            Ok(_status) => {}
            Err(e) => eprintln! {"Early termination of process {}", e},
        },
        Err(e) => eprintln!("Failed to spawn the process {}", e),
    }
}

fn main() {
    let pathenv = env::var("PATH").unwrap();

    loop {
        prompt();

        let command = read_input();

        let words = tokenize(&command.trim());

        if let Some(parsed_command) = ParsedCommand::new(words) {
            if let ControlFlow::Break(_) = dispatch_command(&pathenv, parsed_command) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tilde() {
        let command = "cd ~/Downloads/books";
        assert_eq!(
            tokenize(command),
            Vec::from([
                String::from("cd"),
                String::from("/Users/nashjr/Downloads/books")
            ])
        );
    }

    #[test]
    fn test_backslash() {
        let command = "cd \\~/Downloads/books";
        assert_eq!(
            tokenize(command),
            Vec::from([String::from("cd"), String::from("~/Downloads/books")])
        );
    }
}
