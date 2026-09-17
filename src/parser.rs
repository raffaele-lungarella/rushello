enum Quote {
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
}

pub fn parse(line: &str) -> Result<Vec<String>, ParseError> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut in_quotes: Option<Quote> = None;
    let mut token_started = false;

    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match (character, &in_quotes) {
            ('\\', None) => {
                let escaped = characters.next().ok_or(ParseError::TrailingBackslash)?;
                if escaped != '\n' {
                    current_token.push(escaped);
                    token_started = true;
                }
            }
            ('\\', Some(Quote::Double)) => match characters.peek() {
                Some('"' | '\\' | '$' | '`' | '\n') => {
                    let escaped = characters.next().unwrap();
                    if escaped != '\n' {
                        current_token.push(escaped);
                    }
                }
                _ => current_token.push('\\'),
            },
            ('\'', Some(Quote::Single)) => in_quotes = None,
            ('"', Some(Quote::Double)) => in_quotes = None,
            ('\'', None) => {
                in_quotes = Some(Quote::Single);
                token_started = true;
            }
            ('"', None) => {
                in_quotes = Some(Quote::Double);
                token_started = true;
            }
            (character, None) if character.is_whitespace() => {
                if token_started {
                    tokens.push(std::mem::take(&mut current_token));
                    token_started = false;
                }
            }
            (character, _) => {
                current_token.push(character);
                token_started = true;
            }
        }
    }

    match in_quotes {
        Some(Quote::Single) => return Err(ParseError::UnclosedSingleQuote),
        Some(Quote::Double) => return Err(ParseError::UnclosedDoubleQuote),
        None => {}
    }

    if token_started {
        tokens.push(current_token);
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::{ParseError, parse};

    #[test]
    fn escapes_characters_outside_quotes() {
        assert_eq!(
            parse(r#"echo hello\ world \'quoted\' \"double\" \\ \a"#).unwrap(),
            ["echo", "hello world", "'quoted'", "\"double\"", "\\", "a"]
        );
    }

    #[test]
    fn preserves_backslashes_inside_single_quotes() {
        assert_eq!(
            parse(r#"echo 'hello\ world\\'"#).unwrap(),
            ["echo", r"hello\ world\\"]
        );
        assert_eq!(parse(r"echo '\' next").unwrap(), ["echo", "\\", "next"]);
    }

    #[test]
    fn escapes_special_characters_inside_double_quotes() {
        assert_eq!(
            parse(r#"echo "say \"hello\" \\ \$ \`""#).unwrap(),
            ["echo", "say \"hello\" \\ $ `"]
        );
    }

    #[test]
    fn preserves_other_backslashes_inside_double_quotes() {
        assert_eq!(
            parse(r#"echo "hello\ world\n\'""#).unwrap(),
            ["echo", r"hello\ world\n\'"]
        );
    }

    #[test]
    fn removes_escaped_newlines_outside_single_quotes() {
        assert_eq!(parse("echo hel\\\nlo").unwrap(), ["echo", "hello"]);
        assert_eq!(parse("echo \"hel\\\nlo\"").unwrap(), ["echo", "hello"]);
        assert!(parse("\\\n").unwrap().is_empty());
        assert_eq!(parse("echo 'hel\\\nlo'").unwrap(), ["echo", "hel\\\nlo"]);
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
        assert_eq!(parse("echo").unwrap(), ["echo"]);
    }

    #[test]
    fn tokenizes_multiple_words() {
        assert_eq!(
            parse("echo hello world").unwrap(),
            ["echo", "hello", "world"]
        );
    }

    #[test]
    fn ignores_extra_whitespace() {
        assert_eq!(parse("  echo   hello  ").unwrap(), ["echo", "hello"]);
    }

    #[test]
    fn keeps_single_quoted_text_in_one_token() {
        assert_eq!(
            parse("echo 'hello world'").unwrap(),
            ["echo", "hello world"]
        );
    }

    #[test]
    fn keeps_double_quoted_text_in_one_token() {
        assert_eq!(
            parse(r#"echo "hello world""#).unwrap(),
            ["echo", "hello world"]
        );
    }

    #[test]
    fn preserves_whitespace_inside_double_quotes() {
        assert_eq!(
            parse("echo \"  hello\t world  \"").unwrap(),
            ["echo", "  hello\t world  "]
        );
    }

    #[test]
    fn separates_arguments_after_double_quotes() {
        assert_eq!(
            parse(r#"echo "hello world" next"#).unwrap(),
            ["echo", "hello world", "next"]
        );
    }

    #[test]
    fn combines_double_quoted_and_unquoted_parts() {
        assert_eq!(
            parse(r#"echo before" middle "after"#).unwrap(),
            ["echo", "before middle after"]
        );
    }

    #[test]
    fn preserves_single_quotes_inside_double_quotes() {
        assert_eq!(
            parse(r#"echo "it's 'quoted'""#).unwrap(),
            ["echo", "it's 'quoted'"]
        );
    }

    #[test]
    fn preserves_double_quotes_inside_single_quotes() {
        assert_eq!(
            parse(r#"echo 'say "hello"'"#).unwrap(),
            ["echo", "say \"hello\""]
        );
    }

    #[test]
    fn combines_adjacent_single_and_double_quoted_parts() {
        assert_eq!(
            parse(r#"echo 'hello '"world""#).unwrap(),
            ["echo", "hello world"]
        );
    }

    #[test]
    fn preserves_empty_double_quoted_arguments() {
        assert_eq!(
            parse(r#"echo "" next """#).unwrap(),
            ["echo", "", "next", ""]
        );
    }

    #[test]
    fn preserves_empty_single_quoted_arguments() {
        assert_eq!(parse("echo '' next ''").unwrap(), ["echo", "", "next", ""]);
    }

    #[test]
    fn ignores_empty_and_whitespace_only_input() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse(" \t\n ").unwrap().is_empty());
    }

    #[test]
    fn combines_adjacent_empty_quotes_into_one_token() {
        assert_eq!(parse(r#"  ''""   ""hello''  "#).unwrap(), ["", "hello"]);
    }

    #[test]
    fn combines_quoted_and_unquoted_parts_of_a_token() {
        assert_eq!(
            parse("echo hello' world'").unwrap(),
            ["echo", "hello world"]
        );
    }
}
