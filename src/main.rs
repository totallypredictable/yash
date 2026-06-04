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

        if words[0].trim() == "exit" {
            break;
        } else if words[0] == "echo" {
            println!("{}", words[1..].join(" "));
        } else {
            println!("{}: command not found", command.trim());
        }
    }
}
