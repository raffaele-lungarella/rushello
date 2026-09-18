enum QuoteState {
    Unquoted,
    Single,
    Double,
}

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

#[derive(Debug, PartialEq, Eq)]
pub enum Token {
    Word(String),
    RedirectOut,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ParsedCommand {
    pub args: Vec<String>,
    pub stdout_redirects: Vec<String>,
}

pub fn parse(line: &str) -> Result<ParsedCommand, ParseError> {
    let mut tokens = tokenize(line)?.into_iter();
    let mut command = ParsedCommand::default();

    while let Some(token) = tokens.next() {
        match token {
            Token::Word(word) => command.args.push(word),
            Token::RedirectOut => match tokens.next() {
                Some(Token::Word(target)) => command.stdout_redirects.push(target),
                _ => return Err(ParseError::MissingRedirectTarget),
            },
        }
    }

    Ok(command)
}

pub fn tokenize(line: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut state = QuoteState::Unquoted;
    let mut token_started = false;

    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match (character, &state) {
            ('\\', QuoteState::Unquoted) => {
                let escaped = characters.next().ok_or(ParseError::TrailingBackslash)?;
                if escaped != '\n' {
                    current_token.push(escaped);
                    token_started = true;
                }
            }
            ('\\', QuoteState::Double) => match characters.peek() {
                Some('"' | '\\' | '$' | '`' | '\n') => {
                    let escaped = characters.next().unwrap();
                    if escaped != '\n' {
                        current_token.push(escaped);
                    }
                }
                _ => current_token.push('\\'),
            },
            ('\'', QuoteState::Single) => state = QuoteState::Unquoted,
            ('"', QuoteState::Double) => state = QuoteState::Unquoted,
            ('\'', QuoteState::Unquoted) => {
                state = QuoteState::Single;
                token_started = true;
            }
            ('"', QuoteState::Unquoted) => {
                state = QuoteState::Double;
                token_started = true;
            }
            ('>', QuoteState::Unquoted) => {
                if token_started {
                    tokens.push(Token::Word(std::mem::take(&mut current_token)));
                    token_started = false;
                }
                tokens.push(Token::RedirectOut);
            }
            (character, QuoteState::Unquoted) if character.is_whitespace() => {
                if token_started {
                    tokens.push(Token::Word(std::mem::take(&mut current_token)));
                    token_started = false;
                }
            }
            (character, _) => {
                current_token.push(character);
                token_started = true;
            }
        }
    }

    match state {
        QuoteState::Single => return Err(ParseError::UnclosedSingleQuote),
        QuoteState::Double => return Err(ParseError::UnclosedDoubleQuote),
        QuoteState::Unquoted => {}
    }

    if token_started {
        tokens.push(Token::Word(current_token));
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{ParseError, Token, parse, tokenize};

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
            assert_eq!(command.stdout_redirects, ["out"]);
        }
    }

    #[test]
    fn parses_quoted_redirect_targets() {
        let command = parse(r#"echo >"my file" hello"#).unwrap();
        assert_eq!(command.args, ["echo", "hello"]);
        assert_eq!(command.stdout_redirects, ["my file"]);
    }

    #[test]
    fn preserves_redirect_order() {
        assert_eq!(
            parse("echo >first >second").unwrap().stdout_redirects,
            ["first", "second"]
        );
    }

    #[test]
    fn rejects_missing_redirect_targets() {
        for input in ["echo >", "echo > >out", "echo >>out"] {
            assert_eq!(parse(input), Err(ParseError::MissingRedirectTarget));
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
