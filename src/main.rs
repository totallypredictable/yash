use libc::{read, tcgetattr, tcsetattr};
use std::collections::{self, HashMap};
use std::env;
use std::fs::{self};
use std::io::{self, Write};
use std::ops::ControlFlow;
use std::os::unix::{fs::PermissionsExt, process::CommandExt};
use std::path::{Path, PathBuf};
use std::process;

const BUILTINS: &[&str] = &["exit", "echo", "type", "pwd", "cd", "complete"];

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

fn read_input(root: &TrieNode, complete_db: &HashMap<String, Vec<String>>) -> String {
    let mut termios: libc::termios = unsafe { std::mem::zeroed() };
    unsafe { tcgetattr(0, &mut termios) };
    let orig_termios = termios; // C structs in libc implement Copy
    termios.c_lflag &= !(libc::ICANON | libc::ECHO);
    unsafe { tcsetattr(0, libc::TCSANOW, &termios) };

    let mut buf: String = String::new();
    let mut byte = [0u8; 1];
    let mut tab = false;

    loop {
        unsafe { read(0, byte.as_mut_ptr() as *mut libc::c_void, 1) };
        match byte[0] {
            0x0a => break,
            0x09 => {
                let args = tokenize(&buf);
                let mut results;
                let completion_prefix: String;
                if args.len() == 1 && !buf.ends_with(' ') {
                    completion_prefix = buf.clone();
                    results = root.search(&buf);
                } else {
                    let full_path;
                    if buf.ends_with(' ') {
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
                        env_vars.push(("COMP_LINE".to_string(), buf.clone()));
                        env_vars.push(("COMP_POINT".to_string(), buf.len().to_string()));
                        let output =
                            run_completer_script(Path::new(&value[0]), &args, env_vars.clone());
                        let stdout_result = String::from_utf8(output.stdout).unwrap();
                        eprintln!("STDOUT_STRING: {}", stdout_result);
                        let outputs = stdout_result.trim().split('\n');

                        for output in outputs {
                            if output.starts_with(&completion_prefix) {
                                tmp.push(output.to_owned());
                            }
                        }
                    } else {
                        let files = search_dir(&full_path);

                        for file in &files {
                            if file.starts_with(&completion_prefix) {
                                tmp.push(file.to_owned());
                            }
                        }
                    }

                    results = tmp;
                }

                results.sort();

                if results.is_empty() {
                    print!("\x07");
                    io::stdout().flush().unwrap();
                    continue;
                }
                if results.len() == 1 {
                    buf.push_str(&results[0][completion_prefix.len()..]);
                    print!("{}", &results[0][completion_prefix.len()..]);
                    if !buf.ends_with("/") {
                        buf.push(' ');
                        print!(" ");
                    }
                    io::stdout().flush().unwrap();
                    continue;
                }
                if results.len() > 1 {
                    let lcp = lcp(&results);

                    if lcp.len() > completion_prefix.len() {
                        print!("{}", &lcp[completion_prefix.len()..]);
                        buf.push_str(&lcp[completion_prefix.len()..]);
                        io::stdout().flush().unwrap();
                        continue;
                    }
                    if !tab {
                        print!("\x07");
                        io::stdout().flush().unwrap();
                        tab = true;
                        continue;
                    } else {
                        println!("\n{}", results.join("  "));
                        prompt();
                        print!("{buf}");
                        io::stdout().flush().unwrap();
                        tab = false;
                        continue;
                    }
                }
            }
            0x7f => {
                if !buf.is_empty() {
                    print!("\x08 \x08");
                    buf.pop();
                    io::stdout().flush().unwrap();
                }
                tab = false;
            }
            _ => {
                buf.push(byte[0] as char);
                print!("{}", byte[0] as char);
                io::stdout().flush().unwrap();
                tab = false;
            }
        }
    }

    unsafe { tcsetattr(0, libc::TCSANOW, &orig_termios) };
    println!();
    buf
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

fn make_handle(redirect: &Option<Redirect>) -> Option<fs::File> {
    match redirect {
        Some(r) => Some(match r.mode {
            FileMode::Truncate => fs::File::create(&r.path).unwrap(),
            FileMode::Append => fs::File::options()
                .append(true)
                .create(true)
                .open(&r.path)
                .unwrap(),
        }),
        None => None,
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
    // TODO: need a way to indicate if args[0] was complete or not
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
    Complete(Vec<String>),
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
            "complete" => Command::Complete(remaining_tokens),
            _ => Command::External(first_token, remaining_tokens),
        };
        Some(ParsedCommand {
            cmd: command_type,
            stdout_redirect,
            stderr_redirect,
        })
    }
}

fn dispatch_command(
    pathenv: &str,
    parsed_command: ParsedCommand,
    complete_db: &mut HashMap<String, Vec<String>>,
) -> ControlFlow<()> {
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
        Command::Complete(args) => {
            let mut stdout_writer = make_writer(&parsed_command.stdout_redirect);
            let mut stderr_writer = make_writer(&parsed_command.stderr_redirect); // creates file if needed
            let key = args.last().unwrap();
            match args[0].as_str() {
                "-p" => {
                    if let Some(value) = complete_db.get(key) {
                        writeln!(stdout_writer, "complete -C '{}' {}", value[0], key).unwrap();
                    } else {
                        writeln!(
                            stderr_writer,
                            "complete: {}: no completion specification",
                            args[1]
                        )
                        .unwrap();
                    }
                }
                "-C" => {
                    if let Some(value) = complete_db.get_mut(key) {
                        value.push(args[args.len() - 2].clone());
                    } else {
                        complete_db.insert(key.clone(), vec![args[args.len() - 2].clone()]);
                    }
                }
                _ => {
                    unimplemented!();
                }
            }

            ControlFlow::Continue(())
        }
        Command::External(bin, args) => {
            let mut stderr_writer = make_writer(&parsed_command.stderr_redirect);
            let stderr_file = make_handle(&parsed_command.stderr_redirect);
            if let Some(path) = resolve_path(pathenv, bin.as_str()) {
                match parsed_command.stdout_redirect {
                    Some(r) => {
                        let stdout_file = make_handle(&Some(r));
                        if let Some(f) = stdout_file {
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

fn run_completer_script(
    path: &Path,
    args: &Vec<String>,
    env_vars: Vec<(String, String)>,
) -> std::process::Output {
    let args_list: Vec<String>;
    if args.len() == 2 {
        args_list = vec![args[0].clone(), String::from(""), args[1].clone()];
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

struct TrieNode {
    children: HashMap<char, Box<TrieNode>>,
    terminal: bool,
}

impl TrieNode {
    fn new() -> TrieNode {
        TrieNode {
            children: HashMap::new(),
            terminal: false,
        }
    }

    fn insert(&mut self, text: String) {
        let mut tmp: &mut TrieNode = self;
        for ch in (&text).chars() {
            if !tmp.children.contains_key(&ch) {
                tmp.children.insert(ch, Box::from(Self::new()));
            }

            tmp = &mut *tmp.children.get_mut(&ch).unwrap();
        }

        tmp.terminal = true;
    }

    fn search(&self, text: &str) -> Vec<String> {
        let mut tmp: &TrieNode = self;
        let mut results: Vec<String> = Vec::new();
        for ch in (&text).chars() {
            if tmp.children.contains_key(&ch) {
                tmp = &*tmp.children.get(&ch).unwrap();
            } else {
                return results;
            }
        }
        tmp.rec_search(&(text.to_string()), &mut results);

        return results;
    }

    fn rec_search(&self, text: &String, results: &mut Vec<String>) {
        if self.terminal {
            results.push(text.to_string());
        }
        for key in self.children.keys() {
            TrieNode::rec_search(
                &*self.children[key],
                &(text.to_string() + &key.to_string()),
                results,
            )
        }
    }
}

fn build_exec_db(pathenv: &str, root: &mut TrieNode) {
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

fn main() {
    let pathenv = env::var("PATH").unwrap_or_else(|_| String::from("/usr/local/bin:/usr/bin:/bin"));

    let mut root = TrieNode::new();

    build_exec_db(&pathenv, &mut root);

    let mut complete_db = HashMap::new();

    loop {
        prompt();

        let command = read_input(&root, &complete_db);

        let words = tokenize(&command.trim());

        if let Some(parsed_command) = ParsedCommand::new(words) {
            if let ControlFlow::Break(_) =
                dispatch_command(&pathenv, parsed_command, &mut complete_db)
            {
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

    #[test]
    fn test_tokenize() {
        assert_eq!(tokenize("ls "), ["ls".to_string()]);
        assert_eq!(
            tokenize("complete -p git"),
            ["complete".to_string(), "-p".to_string(), "git".to_string()]
        );
    }
}
