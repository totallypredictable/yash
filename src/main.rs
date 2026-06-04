use std::env;
use std::fs;
#[allow(unused_imports)]
use std::io::{self, Write};
use std::ops::ControlFlow;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
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

fn tokenize(command: &str) -> Vec<&str> {
    let words: Vec<&str> = command.trim().split_whitespace().collect();
    words
}

struct ParsedCommand<'a> {
    cmd: Command<'a>,
}

impl<'a> ParsedCommand<'a> {
    fn new(words: &'a [&'a str]) -> Option<Self> {
        if let [first_token, remaining_tokens @ ..] = words {
            let command_type = match *first_token {
                "exit" => Command::Exit,
                "echo" => Command::Echo(remaining_tokens),
                "type" => Command::Type(remaining_tokens),
                "pwd" => Command::Pwd,
                "cd" => Command::Cd(remaining_tokens),
                _ => Command::External(first_token, remaining_tokens),
            };
            Some(ParsedCommand { cmd: command_type })
        } else {
            return None;
        }
    }
}

enum Command<'a> {
    Exit,
    Echo(&'a [&'a str]),
    Type(&'a [&'a str]),
    Pwd,
    Cd(&'a [&'a str]),
    External(&'a str, &'a [&'a str]),
}

fn dispatch_command(pathenv: &str, parsed_command: ParsedCommand<'_>) -> ControlFlow<()> {
    match parsed_command.cmd {
        Command::Exit => ControlFlow::Break(()),
        Command::Echo(args) => {
            println!("{}", args.join(" "));
            ControlFlow::Continue(())
        }
        Command::Type(cmds) => {
            for cmd in cmds {
                if BUILTINS.contains(cmd) {
                    println!("{} is a shell builtin", cmd);
                } else if let Some(path) = resolve_path(pathenv, cmd) {
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
            if let Err(e) = env::set_current_dir(Path::new(path[0])) {
                eprintln!("cd: {e}: No such file or directory")
            }
            ControlFlow::Continue(())
        }
        Command::External(bin, args) => {
            if let Some(path) = resolve_path(pathenv, bin) {
                run_program(&path, bin, args);
                ControlFlow::Continue(())
            } else {
                println!("{}: command not found", bin);
                ControlFlow::Continue(())
            }
        }
    }
}

fn run_program(path: &Path, bin: &str, args: &[&str]) {
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

        let words = tokenize(&command);

        if let Some(parsed_command) = ParsedCommand::new(&words) {
            if let ControlFlow::Break(_) = dispatch_command(&pathenv, parsed_command) {
                break;
            }
        }
    }
}
