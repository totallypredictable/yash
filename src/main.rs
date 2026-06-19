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

fn make_writer(redirect: &Option<Redirect>) -> Box<dyn io::Write> {
    match redirect {
        Some(r) => match r.mode {
            FileMode::Truncate => Box::new(fs::File::create(&r.path).unwrap()),
            FileMode::Append => Box::new(
                fs::File::options()
                    .append(true)
                    .create(true)
                    .open(&r.path)
                    .unwrap(),
            ),
        },
        None => Box::new(io::stdout()),
    }
}

enum Backslash {
    Yes,
    No,
}

enum FileMode {
    Truncate,
    Append,
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
    let mut has_quotes = false;

    for ch in command.chars() {
        state = match state {
            TokenizerState::Normal(Backslash::No) => match ch {
                ' ' => {
                    if !buffer.is_empty() || has_quotes {
                        args.push(std::mem::take(&mut buffer));
                        has_quotes = false;
                    }
                    TokenizerState::Normal(Backslash::No)
                }
                '\\' => TokenizerState::Normal(Backslash::Yes),
                '"' => TokenizerState::InDoubleQuote(Backslash::No),
                '\'' => TokenizerState::InSingleQuote,
                '~' => {
                    if !buffer.is_empty() {
                        buffer.push(ch);
                    } else if let Ok(home) = env::var("HOME") {
                        buffer.push_str(&home);
                    } else {
                        buffer.push(ch);
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
                '"' => {
                    has_quotes = true;
                    TokenizerState::Normal(Backslash::No)
                }
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
                '\'' => {
                    has_quotes = true;
                    TokenizerState::Normal(Backslash::No)
                }
                _ => {
                    buffer.push(ch);
                    TokenizerState::InSingleQuote
                }
            },
        };
    }

    if !buffer.is_empty() || has_quotes {
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
    stdout_redirect: Option<Redirect>,
    stderr_redirect: Option<Redirect>,
}

struct Redirect {
    path: PathBuf,
    mode: FileMode,
}

impl ParsedCommand {
    fn new(words: Vec<String>) -> Option<Self> {
        if words.is_empty() {
            return None;
        }
        let mut words = collections::VecDeque::from(words);
        let first_token: String = words.pop_front().unwrap();
        let mut stdout_redirect: Option<Redirect> = None;
        let mut stderr_redirect: Option<Redirect> = None;
        let mut remaining_tokens: Vec<String> = words.into_iter().collect();
        if let Some(idx) = (&remaining_tokens)
            .iter()
            .position(|n| n == ">" || n == "1>")
        {
            match remaining_tokens.get(idx + 1) {
                Some(value) => {
                    stdout_redirect = Some(Redirect {
                        path: PathBuf::from(value),
                        mode: FileMode::Truncate,
                    });
                    remaining_tokens.remove(idx + 1);
                    remaining_tokens.remove(idx);
                }
                None => {
                    eprintln!("syntax error: expected filename after `>`");
                    return None;
                }
            }
        }
        if let Some(idx) = (&remaining_tokens)
            .iter()
            .position(|n| n == ">>" || n == "1>>")
        {
            match remaining_tokens.get(idx + 1) {
                Some(value) => {
                    stdout_redirect = Some(Redirect {
                        path: PathBuf::from(value),
                        mode: FileMode::Append,
                    });
                    remaining_tokens.remove(idx + 1);
                    remaining_tokens.remove(idx);
                }
                None => {
                    eprintln!("syntax error: expected filename after `>>`");
                    return None;
                }
            }
        }
        if let Some(idx) = (&remaining_tokens).iter().position(|n| n == "2>") {
            match remaining_tokens.get(idx + 1) {
                Some(value) => {
                    stderr_redirect = Some(Redirect {
                        path: PathBuf::from(value),
                        mode: FileMode::Truncate,
                    });
                    remaining_tokens.remove(idx + 1);
                    remaining_tokens.remove(idx);
                }
                None => {
                    eprintln!("syntax error: expected filename after `2>`");
                    return None;
                }
            }
        }
        if let Some(idx) = (&remaining_tokens).iter().position(|n| n == "2>>") {
            match remaining_tokens.get(idx + 1) {
                Some(value) => {
                    stderr_redirect = Some(Redirect {
                        path: PathBuf::from(value),
                        mode: FileMode::Append,
                    });
                    remaining_tokens.remove(idx + 1);
                    remaining_tokens.remove(idx);
                }
                None => {
                    eprintln!("syntax error: expected filename after `2>>`");
                    return None;
                }
            }
        }
        let command_type = match first_token.as_str() {
            "exit" => Command::Exit,
            "echo" => Command::Echo(remaining_tokens),
            "type" => Command::Type(remaining_tokens),
            "pwd" => Command::Pwd,
            "cd" => Command::Cd(remaining_tokens),
            _ => Command::External(first_token, remaining_tokens),
        };
        Some(ParsedCommand {
            cmd: command_type,
            stdout_redirect,
            stderr_redirect,
        })
    }
}

fn dispatch_command(pathenv: &str, parsed_command: ParsedCommand) -> ControlFlow<()> {
    match parsed_command.cmd {
        Command::Exit => ControlFlow::Break(()),
        Command::Echo(args) => {
            let mut stdout_writer = make_writer(&parsed_command.stdout_redirect);
            let _stderr_writer = make_writer(&parsed_command.stderr_redirect); // creates file if needed
            writeln!(stdout_writer, "{}", args.join(" ")).unwrap();
            ControlFlow::Continue(())
        }
        Command::Type(cmds) => {
            let mut stdout_writer = make_writer(&parsed_command.stdout_redirect);
            let _stderr_writer = make_writer(&parsed_command.stderr_redirect); // creates file if needed
            for cmd in cmds {
                if BUILTINS.contains(&&cmd.as_str()) {
                    writeln!(stdout_writer, "{} is a shell builtin", cmd).unwrap();
                } else if let Some(path) = resolve_path(pathenv, cmd.as_str()) {
                    writeln!(stdout_writer, "{} is {}", cmd, path.display()).unwrap();
                } else {
                    writeln!(stdout_writer, "{}: not found", cmd).unwrap();
                }
            }
            ControlFlow::Continue(())
        }
        Command::Pwd => {
            let mut stdout_writer = make_writer(&parsed_command.stdout_redirect);
            let _stderr_writer = make_writer(&parsed_command.stderr_redirect); // creates file if needed
            if let Ok(path) = env::current_dir() {
                writeln!(stdout_writer, "{}", path.display()).unwrap()
            }
            ControlFlow::Continue(())
        }
        Command::Cd(path) => {
            let mut stderr_writer = make_writer(&parsed_command.stderr_redirect);
            let target_path: Option<PathBuf> = match path.first() {
                Some(p) => Some(PathBuf::from(p)),
                None => match env::var("HOME") {
                    Ok(h) => Some(PathBuf::from(h)),
                    Err(_) => {
                        writeln!(stderr_writer, "cd: HOME not set").unwrap();
                        None
                    }
                },
            };

            if let Some(path) = target_path {
                if let Err(_) = env::set_current_dir(&path) {
                    writeln!(
                        stderr_writer,
                        "{}: {}: No such file or directory",
                        "cd",
                        path.display()
                    )
                    .unwrap();
                }
            }

            ControlFlow::Continue(())
        }
        Command::External(bin, args) => {
            let mut stderr_writer = make_writer(&parsed_command.stderr_redirect);
            let stderr_file = match parsed_command.stderr_redirect {
                Some(r) => fs::File::create(&r.path).ok(),
                None => None,
            };
            if let Some(path) = resolve_path(pathenv, bin.as_str()) {
                match parsed_command.stdout_redirect {
                    Some(r) => {
                        if let Ok(f) = fs::File::create(&r.path) {
                            run_program(&path, bin.as_str(), &args, Some(f), stderr_file);
                        } else {
                            writeln!(stderr_writer, "Problem with file.").unwrap();
                        }
                    }

                    None => {
                        run_program(&path, bin.as_str(), &args, None, stderr_file);
                    }
                }
                ControlFlow::Continue(())
            } else {
                writeln!(stderr_writer, "{}: command not found", bin).unwrap();
                ControlFlow::Continue(())
            }
        }
    }
}

fn run_program(
    path: &Path,
    bin: &str,
    args: &[String],
    stdout: Option<fs::File>,
    stderr: Option<fs::File>,
) {
    let mut cmd = process::Command::new(path);
    cmd.arg0(bin);
    cmd.args(args);
    if let Some(file) = stdout {
        cmd.stdout(file);
    }
    if let Some(file) = stderr {
        cmd.stderr(file);
    }
    match cmd.spawn() {
        Ok(mut handle) => match handle.wait() {
            Ok(_status) => {}
            Err(e) => eprintln! {"Early termination of process {}", e},
        },
        Err(e) => eprintln!("Failed to spawn process {}", e),
    }
}

fn main() {
    let pathenv = env::var("PATH").unwrap_or_else(|_| String::from("/usr/local/bin:/usr/bin:/bin"));

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
