//! Tokenizes shell input and separates command arguments from output redirections.
//!
//! [`tokenize`] handles quotes, escapes, `>` / `1>` / `2>`, and `>>` / `1>>` / `2>>`.
//! [`parse`] interprets those tokens without opening files or executing commands.
//!
//! This is a small shell syntax subset: expansion, pipelines, other file descriptors,
//! and input redirection are not implemented. Unsupported shell syntax is
//! not necessarily rejected; it may be interpreted as ordinary words.

/// Determines whether whitespace, quotes, and backslashes have special meaning.
enum QuoteState {
    None,
    Single,
    Double,
}

/// Invalid or incomplete syntax encountered during tokenization or parsing.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("unclosed single quote")]
    UnclosedSingleQuote,
    #[error("unclosed double quote")]
    UnclosedDoubleQuote,
    #[error("trailing backslash")]
    TrailingBackslash,
    #[error("missing redirect target")]
    MissingRedirectTarget,
}

/// A word with quoting/escaping processed, or a recognized shell operator.
#[derive(Debug, PartialEq, Eq)]
pub enum Token {
    /// May be empty when the input contains `''` or `""`.
    Word(String),
    /// Both `>` and `1>` redirect stdout and produce this token.
    RedirectOut,
    /// Both `>>` and `1>>` append to stdout's destination.
    AppendOut,
    /// '2>' produces this token.
    RedirectErr,
    /// `2>>` appends to stderr's destination.
    AppendErr,
}

#[derive(Debug, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Redirection {
    pub stream: OutputStream,
    pub target: String,
    /// Append rather than truncate when opening the destination.
    pub append: bool,
}

/// A simple command ready for the executor to resolve its output destinations.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedCommand {
    /// Command name followed by its arguments, excluding redirection syntax.
    /// Empty for blank input or a command consisting only of redirections.
    pub args: Vec<String>,
    /// Destinations in source order, opened according to their append mode.
    /// The last destination for each stream receives that stream's output.
    pub redirects: Vec<Redirection>,
}

/// Separates words and output redirections, which may appear anywhere in the input.
///
/// For example, `echo >"my file" hello` yields arguments `["echo", "hello"]`
/// and the output target `"my file"`. Empty input produces an empty command.
///
/// # Errors
///
/// Returns tokenization errors or [`ParseError::MissingRedirectTarget`] when a
/// redirection is not followed by a word. A quoted empty filename is a valid word;
/// any resulting file-open error is the executor's responsibility.
pub fn parse(line: &str) -> Result<ParsedCommand, ParseError> {
    let mut tokens = tokenize(line)?.into_iter();
    let mut command = ParsedCommand::default();

    while let Some(token) = tokens.next() {
        match token {
            Token::Word(word) => command.args.push(word),
            Token::RedirectOut => match tokens.next() {
                Some(Token::Word(target)) => command.redirects.push(Redirection {
                    stream: OutputStream::Stdout,
                    append: false,
                    target: target,
                }),
                _ => return Err(ParseError::MissingRedirectTarget),
            },
            Token::AppendOut => match tokens.next() {
                Some(Token::Word(target)) => command.redirects.push(Redirection {
                    stream: OutputStream::Stdout,
                    target,
                    append: true,
                }),
                _ => return Err(ParseError::MissingRedirectTarget),
            },
            Token::AppendErr => match tokens.next() {
                Some(Token::Word(target)) => command.redirects.push(Redirection {
                    stream: OutputStream::Stderr,
                    target,
                    append: true,
                }),
                _ => return Err(ParseError::MissingRedirectTarget),
            },
            Token::RedirectErr => match tokens.next() {
                Some(Token::Word(target)) => command.redirects.push(Redirection {
                    stream: OutputStream::Stderr,
                    append: false,
                    target: target,
                }),
                _ => return Err(ParseError::MissingRedirectTarget),
            },
        }
    }

    Ok(command)
}

/// Splits input into words and output-redirection operators in one pass.
///
/// Unquoted Unicode whitespace separates words. Quoted and unquoted fragments
/// of a word are joined, and empty quoted words are preserved. Quoted or escaped
/// `>` characters remain literal; operators do not require surrounding spaces.
/// Backslash-newline pairs are removed outside single quotes, but this function
/// does not read additional input to complete a line yet.
///
/// # Errors
///
/// Returns an error for unclosed quotes or a trailing unquoted backslash.
/// Redirection targets are validated by [`parse`], not by this function.
pub fn tokenize(line: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut state = QuoteState::None;
    // An empty quoted word is still a token; String::is_empty cannot distinguish it
    // from whitespace between words.
    let mut token_started = false;

    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match (character, &state) {
            ('\\', QuoteState::None) => {
                let escaped = characters.next().ok_or(ParseError::TrailingBackslash)?;
                if escaped != '\n' {
                    current_token.push(escaped);
                    token_started = true;
                }
            }
            // Inside double quotes, only these characters consume the backslash.
            // peek leaves other characters for the next loop iteration.
            ('\\', QuoteState::Double) => match characters.peek() {
                Some('"' | '\\' | '$' | '`' | '\n') => {
                    let escaped = characters.next().unwrap();
                    if escaped != '\n' {
                        current_token.push(escaped);
                    }
                }
                _ => current_token.push('\\'),
            },
            ('\'', QuoteState::Single) => state = QuoteState::None,
            ('"', QuoteState::Double) => state = QuoteState::None,
            ('\'', QuoteState::None) => {
                state = QuoteState::Single;
                token_started = true;
            }
            ('"', QuoteState::None) => {
                state = QuoteState::Double;
                token_started = true;
            }

            // `1>` explicitly selects stdout; the unquoted 1 must start a word.
            // In `hello1>out` or `"1">out`, the 1 belongs to the argument instead.
            ('1', QuoteState::None) if !token_started && characters.peek() == Some(&'>') => {
                characters.next();
                if characters.next_if_eq(&'>').is_some() {
                    tokens.push(Token::AppendOut);
                } else {
                    tokens.push(Token::RedirectOut);
                }
            }
            ('2', QuoteState::None) if !token_started && characters.peek() == Some(&'>') => {
                characters.next();
                if characters.next_if_eq(&'>').is_some() {
                    tokens.push(Token::AppendErr);
                } else {
                    tokens.push(Token::RedirectErr);
                }
            }
            // Finish an adjacent word before emitting the operator: hello>out.
            ('>', QuoteState::None) => {
                if token_started {
                    tokens.push(Token::Word(std::mem::take(&mut current_token)));
                    token_started = false;
                }
                if characters.next_if_eq(&'>').is_some() {
                    tokens.push(Token::AppendOut);
                } else {
                    tokens.push(Token::RedirectOut);
                }
            }
            (character, QuoteState::None) if character.is_whitespace() => {
                if token_started {
                    tokens.push(Token::Word(std::mem::take(&mut current_token)));
                    token_started = false;
                }
            }
            // Includes backslashes inside single quotes and nonmatching quote
            // characters, such as a single quote inside double quotes.
            (character, _) => {
                current_token.push(character);
                token_started = true;
            }
        }
    }

    match state {
        QuoteState::Single => return Err(ParseError::UnclosedSingleQuote),
        QuoteState::Double => return Err(ParseError::UnclosedDoubleQuote),
        QuoteState::None => {}
    }

    if token_started {
        tokens.push(Token::Word(current_token));
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use crate::parser::OutputStream;
    use crate::parser::Redirection;

    use super::{ParseError, Token, parse, tokenize};

    #[test]
    fn parses_stderr_append_redirects() {
        for input in [
            "echo hello 2>> err",
            "echo 2>>err hello",
            "2>>err echo hello",
        ] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", "hello"]);
            assert_eq!(
                command.redirects,
                [Redirection {
                    stream: OutputStream::Stderr,
                    target: "err".into(),
                    append: true,
                }]
            );
        }
        assert_eq!(
            tokenize("2>>err").unwrap(),
            [Token::AppendErr, Token::Word("err".into())]
        );
    }

    #[test]
    fn preserves_literal_stderr_append_operators() {
        for input in ["echo '2>>'", r#"echo "2>>""#, r"echo 2\>\>"] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", "2>>"]);
            assert!(command.redirects.is_empty());
        }
        for (input, argument) in [
            ("echo hello2>>out", "hello2"),
            ("echo 2 >>out", "2"),
            ("echo '2'>>out", "2"),
            (r"echo \2>>out", "2"),
        ] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", argument]);
            assert_eq!(
                command.redirects,
                [Redirection {
                    stream: OutputStream::Stdout,
                    target: "out".into(),
                    append: true,
                }]
            );
        }
    }

    #[test]
    fn parses_mixed_stderr_append_redirects_in_order() {
        let command = parse(r#"echo 2>>"first error" >out 2>last"#).unwrap();
        assert_eq!(
            command.redirects,
            [
                Redirection {
                    stream: OutputStream::Stderr,
                    target: "first error".into(),
                    append: true
                },
                Redirection {
                    stream: OutputStream::Stdout,
                    target: "out".into(),
                    append: false
                },
                Redirection {
                    stream: OutputStream::Stderr,
                    target: "last".into(),
                    append: false
                },
            ]
        );
    }

    #[test]
    fn rejects_missing_stderr_append_targets() {
        for input in ["echo 2>>", "echo 2>>>err", "echo 2>> >out"] {
            assert_eq!(parse(input), Err(ParseError::MissingRedirectTarget));
        }
    }

    #[test]
    fn parses_stdout_append_redirects() {
        for input in [
            "echo hello>>out",
            "echo hello 1>>out",
            "1>>out echo hello",
            "echo >>out hello",
        ] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", "hello"]);
            assert_eq!(
                command.redirects,
                [Redirection {
                    stream: OutputStream::Stdout,
                    target: "out".into(),
                    append: true,
                }]
            );
        }
        assert_eq!(
            tokenize("1>>out").unwrap(),
            [Token::AppendOut, Token::Word("out".into())]
        );
    }

    #[test]
    fn preserves_literal_append_operators() {
        for input in [r#"echo ">>""#, "echo '>>'", r"echo \>\>"] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", ">>"]);
            assert!(command.redirects.is_empty());
        }
        for (input, argument) in [
            ("echo hello1>>out", "hello1"),
            ("echo '1'>>out", "1"),
            (r"echo \1>>out", "1"),
        ] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", argument]);
            assert!(command.redirects[0].append);
        }
    }

    #[test]
    fn preserves_mixed_append_and_truncate_order() {
        let command = parse(r#"echo >>"first file" >second 1>>third 2>errors"#).unwrap();
        assert_eq!(
            command.redirects,
            [
                Redirection {
                    stream: OutputStream::Stdout,
                    target: "first file".into(),
                    append: true
                },
                Redirection {
                    stream: OutputStream::Stdout,
                    target: "second".into(),
                    append: false
                },
                Redirection {
                    stream: OutputStream::Stdout,
                    target: "third".into(),
                    append: true
                },
                Redirection {
                    stream: OutputStream::Stderr,
                    target: "errors".into(),
                    append: false
                },
            ]
        );
    }

    #[test]
    fn parses_explicit_stdout_redirects() {
        for input in [
            "echo Hello David 1> out",
            "echo Hello David 1>out",
            "1>out echo Hello David",
        ] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", "Hello", "David"]);
            assert_eq!(
                command.redirects,
                [Redirection {
                    stream: OutputStream::Stdout,
                    append: false,
                    target: "out".to_owned(),
                }]
            );
        }
        assert_eq!(parse("echo 1>"), Err(ParseError::MissingRedirectTarget));
    }

    #[test]
    fn preserves_literal_ones_before_redirects() {
        for (input, argument) in [
            ("echo hello1>out", "hello1"),
            ("echo 1 >out", "1"),
            ("echo '1'>out", "1"),
            (r#"echo "1">out"#, "1"),
            (r"echo \1>out", "1"),
            ("echo ''1>out", "1"),
        ] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", argument]);
            assert_eq!(
                command.redirects,
                [Redirection {
                    stream: OutputStream::Stdout,
                    append: false,
                    target: "out".into(),
                }]
            );
        }
    }

    #[test]
    fn preserves_quoted_and_escaped_explicit_redirects() {
        for input in ["echo '1>'", r#"echo "1>""#, r"echo 1\>"] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", "1>"]);
            assert!(command.redirects.is_empty());
        }
    }

    #[test]
    fn tokenizes_redirects_without_spaces() {
        assert_eq!(
            tokenize("echo hello>out").unwrap(),
            [
                Token::Word("echo".into()),
                Token::Word("hello".into()),
                Token::RedirectOut,
                Token::Word("out".into())
            ]
        );
    }

    #[test]
    fn quoted_and_escaped_redirects_are_words() {
        assert_eq!(
            tokenize(r#"'>' ">" \>"#).unwrap(),
            [
                Token::Word(">".into()),
                Token::Word(">".into()),
                Token::Word(">".into())
            ]
        );
    }

    #[test]
    fn parses_redirects_anywhere_in_command() {
        for input in ["echo hello >out", "echo >out hello", ">out echo hello"] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", "hello"]);
            assert_eq!(
                command.redirects,
                [Redirection {
                    stream: OutputStream::Stdout,
                    append: false,
                    target: "out".into(),
                }]
            );
        }
    }

    #[test]
    fn parses_quoted_redirect_targets() {
        let command = parse(r#"echo >"my file" hello"#).unwrap();
        assert_eq!(command.args, ["echo", "hello"]);
        assert_eq!(
            command.redirects,
            [Redirection {
                stream: OutputStream::Stdout,
                append: false,
                target: "my file".into(),
            }]
        );
    }

    #[test]
    fn preserves_redirect_order() {
        assert_eq!(
            parse("echo >first >second").unwrap().redirects,
            [
                Redirection {
                    stream: OutputStream::Stdout,
                    append: false,
                    target: "first".into()
                },
                Redirection {
                    stream: OutputStream::Stdout,
                    append: false,
                    target: "second".into()
                },
            ]
        );
    }

    #[test]
    fn rejects_missing_redirect_targets() {
        for input in [
            "echo >",
            "echo > >out",
            "echo >>",
            "echo 1>>",
            "echo >>>out",
            "echo >> >out",
            "echo 2>",
            "echo 2> >out",
            "echo > 2>err",
        ] {
            assert_eq!(parse(input), Err(ParseError::MissingRedirectTarget));
        }
    }

    #[test]
    fn parses_stderr_redirects() {
        for input in ["echo hello 2> err", "echo 2>err hello", "2>err echo hello"] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", "hello"]);
            assert_eq!(
                command.redirects,
                [Redirection {
                    stream: OutputStream::Stderr,
                    append: false,
                    target: "err".into(),
                }]
            );
        }
        assert_eq!(
            tokenize("2>err").unwrap(),
            [Token::RedirectErr, Token::Word("err".into())]
        );
    }

    #[test]
    fn preserves_mixed_redirect_order() {
        let command = parse(r#"echo 2>"first error" hello >out 2>last"#).unwrap();
        assert_eq!(command.args, ["echo", "hello"]);
        assert_eq!(
            command.redirects,
            [
                Redirection {
                    stream: OutputStream::Stderr,
                    append: false,
                    target: "first error".into()
                },
                Redirection {
                    stream: OutputStream::Stdout,
                    append: false,
                    target: "out".into()
                },
                Redirection {
                    stream: OutputStream::Stderr,
                    append: false,
                    target: "last".into()
                },
            ]
        );
    }

    #[test]
    fn preserves_literal_twos_before_redirects() {
        for (input, argument) in [
            ("echo hello2>out", "hello2"),
            ("echo 2 >out", "2"),
            ("echo '2'>out", "2"),
            (r#"echo "2">out"#, "2"),
            (r"echo \2>out", "2"),
            ("echo ''2>out", "2"),
        ] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", argument]);
            assert_eq!(
                command.redirects,
                [Redirection {
                    stream: OutputStream::Stdout,
                    append: false,
                    target: "out".into(),
                }]
            );
        }
    }

    #[test]
    fn preserves_quoted_and_escaped_stderr_operators() {
        for input in ["echo '2>'", r#"echo "2>""#, r"echo 2\>"] {
            let command = parse(input).unwrap();
            assert_eq!(command.args, ["echo", "2>"]);
            assert!(command.redirects.is_empty());
        }
    }

    #[test]
    fn escapes_characters_outside_quotes() {
        assert_eq!(
            parse(r#"echo hello\ world \'quoted\' \"double\" \\ \a"#)
                .unwrap()
                .args,
            ["echo", "hello world", "'quoted'", "\"double\"", "\\", "a"]
        );
    }

    #[test]
    fn preserves_backslashes_inside_single_quotes() {
        assert_eq!(
            parse(r#"echo 'hello\ world\\'"#).unwrap().args,
            ["echo", r"hello\ world\\"]
        );
        assert_eq!(
            parse(r"echo '\' next").unwrap().args,
            ["echo", "\\", "next"]
        );
    }

    #[test]
    fn escapes_special_characters_inside_double_quotes() {
        assert_eq!(
            parse(r#"echo "say \"hello\" \\ \$ \`""#).unwrap().args,
            ["echo", "say \"hello\" \\ $ `"]
        );
    }

    #[test]
    fn preserves_other_backslashes_inside_double_quotes() {
        assert_eq!(
            parse(r#"echo "hello\ world\n\'""#).unwrap().args,
            ["echo", r"hello\ world\n\'"]
        );
    }

    #[test]
    fn removes_escaped_newlines_outside_single_quotes() {
        assert_eq!(parse("echo hel\\\nlo").unwrap().args, ["echo", "hello"]);
        assert_eq!(parse("echo \"hel\\\nlo\"").unwrap().args, ["echo", "hello"]);
        assert!(parse("\\\n").unwrap().args.is_empty());
        assert_eq!(
            parse("echo 'hel\\\nlo'").unwrap().args,
            ["echo", "hel\\\nlo"]
        );
    }

    #[test]
    fn rejects_trailing_backslash() {
        assert_eq!(parse("echo hello\\"), Err(ParseError::TrailingBackslash));
    }

    #[test]
    fn escaped_quote_does_not_close_double_quotes() {
        assert_eq!(
            parse(r#"echo "hello\""#),
            Err(ParseError::UnclosedDoubleQuote)
        );
        assert_eq!(
            parse("echo \"hello\\"),
            Err(ParseError::UnclosedDoubleQuote)
        );
    }

    #[test]
    fn rejects_unclosed_single_quotes() {
        for input in ["'", "echo 'hello", "echo 'hello\""] {
            assert_eq!(parse(input), Err(ParseError::UnclosedSingleQuote));
        }
    }

    #[test]
    fn rejects_unclosed_double_quotes() {
        for input in ["\"", "echo \"hello", "echo \"hello'"] {
            assert_eq!(parse(input), Err(ParseError::UnclosedDoubleQuote));
        }
    }

    #[test]
    fn tokenizes_a_single_word() {
        assert_eq!(parse("echo").unwrap().args, ["echo"]);
    }

    #[test]
    fn tokenizes_multiple_words() {
        assert_eq!(
            parse("echo hello world").unwrap().args,
            ["echo", "hello", "world"]
        );
    }

    #[test]
    fn ignores_extra_whitespace() {
        assert_eq!(parse("  echo   hello  ").unwrap().args, ["echo", "hello"]);
    }

    #[test]
    fn keeps_single_quoted_text_in_one_token() {
        assert_eq!(
            parse("echo 'hello world'").unwrap().args,
            ["echo", "hello world"]
        );
    }

    #[test]
    fn keeps_double_quoted_text_in_one_token() {
        assert_eq!(
            parse(r#"echo "hello world""#).unwrap().args,
            ["echo", "hello world"]
        );
    }

    #[test]
    fn preserves_whitespace_inside_double_quotes() {
        assert_eq!(
            parse("echo \"  hello\t world  \"").unwrap().args,
            ["echo", "  hello\t world  "]
        );
    }

    #[test]
    fn separates_arguments_after_double_quotes() {
        assert_eq!(
            parse(r#"echo "hello world" next"#).unwrap().args,
            ["echo", "hello world", "next"]
        );
    }

    #[test]
    fn combines_double_quoted_and_unquoted_parts() {
        assert_eq!(
            parse(r#"echo before" middle "after"#).unwrap().args,
            ["echo", "before middle after"]
        );
    }

    #[test]
    fn preserves_single_quotes_inside_double_quotes() {
        assert_eq!(
            parse(r#"echo "it's 'quoted'""#).unwrap().args,
            ["echo", "it's 'quoted'"]
        );
    }

    #[test]
    fn preserves_double_quotes_inside_single_quotes() {
        assert_eq!(
            parse(r#"echo 'say "hello"'"#).unwrap().args,
            ["echo", "say \"hello\""]
        );
    }

    #[test]
    fn combines_adjacent_single_and_double_quoted_parts() {
        assert_eq!(
            parse(r#"echo 'hello '"world""#).unwrap().args,
            ["echo", "hello world"]
        );
    }

    #[test]
    fn preserves_empty_double_quoted_arguments() {
        assert_eq!(
            parse(r#"echo "" next """#).unwrap().args,
            ["echo", "", "next", ""]
        );
    }

    #[test]
    fn preserves_empty_single_quoted_arguments() {
        assert_eq!(
            parse("echo '' next ''").unwrap().args,
            ["echo", "", "next", ""]
        );
    }

    #[test]
    fn ignores_empty_and_whitespace_only_input() {
        assert!(parse("").unwrap().args.is_empty());
        assert!(parse(" \t\n ").unwrap().args.is_empty());
    }

    #[test]
    fn combines_adjacent_empty_quotes_into_one_token() {
        assert_eq!(
            parse(r#"  ''""   ""hello''  "#).unwrap().args,
            ["", "hello"]
        );
    }

    #[test]
    fn combines_quoted_and_unquoted_parts_of_a_token() {
        assert_eq!(
            parse("echo hello' world'").unwrap().args,
            ["echo", "hello world"]
        );
    }
}
