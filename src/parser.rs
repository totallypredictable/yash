use std::collections;
use std::path::PathBuf;

pub enum Command {
    Exit,
    Echo(Vec<String>),
    Type(Vec<String>),
    Pwd,
    Cd(Vec<String>),
    Complete(Vec<String>),
    Jobs,
    External {
        bin: String,
        args: Vec<String>,
        background: bool,
    },
}

pub enum FileMode {
    Truncate,
    Append,
}

pub struct Redirect {
    pub path: PathBuf,
    pub mode: FileMode,
}

pub struct ParsedCommand {
    pub cmd: Command,
    pub stdout_redirect: Option<Redirect>,
    pub stderr_redirect: Option<Redirect>,
}

impl ParsedCommand {
    pub fn new(words: Vec<String>) -> Option<Self> {
        if words.is_empty() {
            return None;
        }
        let mut words = collections::VecDeque::from(words);
        let background: bool;
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
        if let Some(last_value) = remaining_tokens.last()
            && last_value == "&"
        {
            background = true;
            remaining_tokens.pop();
        } else {
            background = false;
        }
        let command_type = match first_token.as_str() {
            "exit" => Command::Exit,
            "echo" => Command::Echo(remaining_tokens),
            "type" => Command::Type(remaining_tokens),
            "pwd" => Command::Pwd,
            "cd" => Command::Cd(remaining_tokens),
            "complete" => Command::Complete(remaining_tokens),
            "jobs" => Command::Jobs,
            _ => Command::External {
                bin: first_token,
                args: remaining_tokens,
                background,
            },
        };
        Some(ParsedCommand {
            cmd: command_type,
            stdout_redirect,
            stderr_redirect,
        })
    }
}
