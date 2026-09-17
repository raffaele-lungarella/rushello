enum Quote {
    Single,
    Double,
}

pub fn parse(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut in_quotes: Option<Quote> = None;
    let mut token_started = false;

    for character in line.chars() {
        match (character, &in_quotes) {
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

    if token_started {
        tokens.push(current_token);
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn tokenizes_a_single_word() {
        assert_eq!(parse("echo"), ["echo"]);
    }

    #[test]
    fn tokenizes_multiple_words() {
        assert_eq!(parse("echo hello world"), ["echo", "hello", "world"]);
    }

    #[test]
    fn ignores_extra_whitespace() {
        assert_eq!(parse("  echo   hello  "), ["echo", "hello"]);
    }

    #[test]
    fn keeps_single_quoted_text_in_one_token() {
        assert_eq!(parse("echo 'hello world'"), ["echo", "hello world"]);
    }

    #[test]
    fn keeps_double_quoted_text_in_one_token() {
        assert_eq!(parse(r#"echo "hello world""#), ["echo", "hello world"]);
    }

    #[test]
    fn preserves_whitespace_inside_double_quotes() {
        assert_eq!(
            parse("echo \"  hello\t world  \""),
            ["echo", "  hello\t world  "]
        );
    }

    #[test]
    fn separates_arguments_after_double_quotes() {
        assert_eq!(
            parse(r#"echo "hello world" next"#),
            ["echo", "hello world", "next"]
        );
    }

    #[test]
    fn combines_double_quoted_and_unquoted_parts() {
        assert_eq!(
            parse(r#"echo before" middle "after"#),
            ["echo", "before middle after"]
        );
    }

    #[test]
    fn preserves_single_quotes_inside_double_quotes() {
        assert_eq!(parse(r#"echo "it's 'quoted'""#), ["echo", "it's 'quoted'"]);
    }

    #[test]
    fn preserves_double_quotes_inside_single_quotes() {
        assert_eq!(parse(r#"echo 'say "hello"'"#), ["echo", "say \"hello\""]);
    }

    #[test]
    fn combines_adjacent_single_and_double_quoted_parts() {
        assert_eq!(parse(r#"echo 'hello '"world""#), ["echo", "hello world"]);
    }

    #[test]
    fn preserves_empty_double_quoted_arguments() {
        assert_eq!(parse(r#"echo "" next """#), ["echo", "", "next", ""]);
    }

    #[test]
    fn preserves_empty_single_quoted_arguments() {
        assert_eq!(parse("echo '' next ''"), ["echo", "", "next", ""]);
    }

    #[test]
    fn ignores_empty_and_whitespace_only_input() {
        assert!(parse("").is_empty());
        assert!(parse(" \t\n ").is_empty());
    }

    #[test]
    fn combines_adjacent_empty_quotes_into_one_token() {
        assert_eq!(parse(r#"  ''""   ""hello''  "#), ["", "hello"]);
    }

    #[test]
    fn combines_quoted_and_unquoted_parts_of_a_token() {
        assert_eq!(parse("echo hello' world'"), ["echo", "hello world"]);
    }
}
