use crate::completion::{CompCandidate, CompletionEngine};
use libc::{read, tcgetattr, tcsetattr};
use std::collections::HashMap;
use std::io::{self, Write};

pub struct Buffer {
    text: String,
    _cursor_at: usize,
}

impl Buffer {
    fn new() -> Self {
        Buffer {
            text: String::new(),
            _cursor_at: 0,
        }
    }
}

struct Termios {
    orig: libc::termios,
}

impl Termios {
    fn enter_raw_mode() -> Self {
        Termios {
            orig: {
                let mut termios: libc::termios = unsafe { std::mem::zeroed() };
                unsafe { tcgetattr(0, &mut termios) };
                let orig_termios = termios;
                termios.c_lflag &= !(libc::ICANON | libc::ECHO);
                unsafe { tcsetattr(0, libc::TCSANOW, &termios) };
                orig_termios
            },
        }
    }
}

impl Drop for Termios {
    fn drop(&mut self) {
        unsafe { tcsetattr(0, libc::TCSANOW, &self.orig) };
    }
}

pub fn prompt() {
    print!("$ ");
    io::stdout().flush().unwrap();
}

pub fn read_input(
    comp_engine: &CompletionEngine,
    complete_db: &HashMap<String, Vec<String>>,
) -> String {
    let _termios = Termios::enter_raw_mode();

    let mut buf = Buffer::new();
    let mut byte = [0u8; 1];
    let mut tab = false;

    loop {
        unsafe { read(0, byte.as_mut_ptr() as *mut libc::c_void, 1) };
        match byte[0] {
            0x0a => break,
            0x09 => match comp_engine.find_matches(&buf.text, &complete_db) {
                CompCandidate::Nothing => {
                    print!("\x07");
                    io::stdout().flush().unwrap();
                    continue;
                }
                CompCandidate::ExpandBuffer {
                    suffix,
                    append_space,
                } => {
                    buf.text.push_str(&suffix);
                    print!("{}", &suffix);
                    if append_space {
                        buf.text.push(' ');
                        print!(" ");
                    }
                    io::stdout().flush().unwrap();
                    continue;
                }
                CompCandidate::ListCandidates { candidates } => {
                    if !tab {
                        print!("\x07");
                        io::stdout().flush().unwrap();
                        tab = true;
                        continue;
                    } else {
                        println!("\n{}", candidates.join("  "));
                        prompt();
                        print!("{}", buf.text);
                        io::stdout().flush().unwrap();
                        tab = false;
                        continue;
                    }
                }
            },
            0x7f => {
                if !buf.text.is_empty() {
                    print!("\x08 \x08");
                    buf.text.pop();
                    io::stdout().flush().unwrap();
                }
                tab = false;
            }
            _ => {
                buf.text.push(byte[0] as char);
                print!("{}", byte[0] as char);
                io::stdout().flush().unwrap();
                tab = false;
            }
        }
    }

    println!();
    buf.text
}
