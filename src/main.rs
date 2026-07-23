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

        let words = tokenize(&command.trim());

        if let Some(parsed_command) = ParsedCommand::new(words) {
            if let ControlFlow::Break(_) = shell.dispatch(parsed_command) {
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
    fn test_tokenize() {
        assert_eq!(tokenize("ls "), ["ls".to_string()]);
        assert_eq!(
            tokenize("complete -p git"),
            ["complete".to_string(), "-p".to_string(), "git".to_string()]
        );
        assert_eq!(
            tokenize(r#"cat "my ""#),
            ["cat".to_string(), "my ".to_string()]
        );
        assert_eq!(
            tokenize(r#"cat "\i""#),
            ["cat".to_string(), r"\i".to_string()]
        );
    }
}
