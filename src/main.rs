use std::ops::ControlFlow;

mod completion;
mod env;
mod exec;
mod line_editor;
mod parser;
mod shell;
mod tokenizer;
mod trie;
use line_editor::prompt;
use parser::ParsedCommand;
use shell::Shell;
use tokenizer::tokenize;
use trie::TrieNode;

fn main() {
    let mut shell = Shell::new();

    loop {
        shell.reap();

        prompt();

        let command = shell.read_line();

        let tokens = tokenize(&command.trim());

        if let Some(parsed_command) = ParsedCommand::new(tokens) {
            if let ControlFlow::Break(_) = shell.dispatch(parsed_command) {
                break;
            }
        }
    }
}
