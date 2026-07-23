use crate::env::{BUILTINS, resolve_path};
use crate::parser::{Command, FileMode, ParsedCommand, Redirect};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::ops::ControlFlow;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process;

pub struct Job {
    id: usize,
    command: String,
    child: process::Child,
}

pub fn dispatch_command(
    pathenv: &str,
    parsed_command: ParsedCommand,
    complete_db: &mut HashMap<String, Vec<String>>,
    jobs: &mut Vec<Job>,
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
            if let Ok(path) = std::env::current_dir() {
                writeln!(stdout_writer, "{}", path.display()).unwrap()
            }
            ControlFlow::Continue(())
        }
        Command::Cd(path) => {
            let mut stderr_writer = make_writer(&parsed_command.stderr_redirect);
            let target_path: Option<PathBuf> = match path.first() {
                Some(p) => Some(PathBuf::from(p)),
                None => match std::env::var("HOME") {
                    Ok(h) => Some(PathBuf::from(h)),
                    Err(_) => {
                        writeln!(stderr_writer, "cd: HOME not set").unwrap();
                        None
                    }
                },
            };

            if let Some(path) = target_path {
                if let Err(_) = std::env::set_current_dir(&path) {
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
                "-r" => {
                    complete_db.remove(key);
                }
                _ => {
                    unimplemented!();
                }
            }

            ControlFlow::Continue(())
        }
        Command::Jobs => {
            reap(jobs);
            let vec_len = jobs.len();
            for (i, job) in jobs.into_iter().enumerate() {
                if i == vec_len - 1 {
                    println!("[{}]{}  {:<24}{}", job.id, "+", "Running", job.command);
                } else if i == vec_len - 2 {
                    println!("[{}]{}  {:<24}{}", job.id, "-", "Running", job.command);
                } else {
                    println!("[{}]{}  {:<24}{}", job.id, " ", "Running", job.command);
                }
            }
            ControlFlow::Continue(())
        }
        Command::External {
            bin,
            args,
            background,
        } => {
            let mut stderr_writer = make_writer(&parsed_command.stderr_redirect);
            let stderr_file = make_handle(&parsed_command.stderr_redirect);
            let stdout_file;
            if let Some(path) = resolve_path(pathenv, bin.as_str()) {
                match parsed_command.stdout_redirect {
                    Some(r) => {
                        stdout_file = make_handle(&Some(r));
                    }

                    None => {
                        stdout_file = make_handle(&None);
                    }
                }
                if let Some(mut child_proc) =
                    spawn(&path, bin.as_str(), &args, stdout_file, stderr_file)
                {
                    if background {
                        let pid = child_proc.id();
                        jobs.push(Job {
                            id: jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1,
                            command: format!("{} {} &", bin, args.join(" ")),
                            child: child_proc,
                        });
                        println!("[{}] {}", jobs.last().unwrap().id, pid);
                    } else {
                        if let Err(e) = child_proc.wait() {
                            eprintln!("{e}");
                        }
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

pub fn reap(jobs: &mut Vec<Job>) {
    jobs.retain_mut(|job| match job.child.try_wait() {
        Ok(Some(_)) => {
            println!("[{}]{}  {:<24}{}", job.id, " ", "Done", job.command);
            false
        }
        Ok(None) => true,
        Err(_) => false,
    });
}

pub fn spawn(
    path: &Path,
    bin: &str,
    args: &[String],
    stdout: Option<fs::File>,
    stderr: Option<fs::File>,
) -> Option<process::Child> {
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
        Ok(handle) => Some(handle),
        Err(e) => {
            eprintln!("Failed to spawn process {}", e);
            None
        }
    }
}

pub fn _run_bg_job() {
    todo!();
}

pub fn make_writer(redirect: &Option<Redirect>) -> Box<dyn io::Write> {
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

pub fn make_handle(redirect: &Option<Redirect>) -> Option<fs::File> {
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
