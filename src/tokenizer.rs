use std::env;

enum Backslash {
    Yes,
    No,
}

enum TokenizerState {
    Normal(Backslash),
    InSingleQuote,
    InDoubleQuote(Backslash),
}

pub fn tokenize(command: &str) -> Vec<String> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            tokenize("cat file.txt | echo"),
            ["cat", "file.txt", "|", "echo"]
        );
        assert_eq!(
            tokenize("cat /tmp/ant/file-32 | wc"),
            ["cat", "/tmp/ant/file-32", "|", "wc"]
        );
    }

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
