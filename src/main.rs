#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    // TODO: Uncomment the code below to pass the first stage

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
                } else {
                    println!("{}: not found", words[1]);
                }
            }
            other => println!("{}: command not found", other.trim()),
        }
    }
}
