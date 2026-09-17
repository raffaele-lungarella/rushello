pub fn parse(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut in_single_quotes = false;

    for character in line.chars() {
        match character {
            '\'' => in_single_quotes = !in_single_quotes,
            character if character.is_whitespace() && !in_single_quotes => {
                if !current_token.is_empty() {
                    tokens.push(std::mem::take(&mut current_token));
                }
            }
            character => current_token.push(character),
        }
    }

    if !current_token.is_empty() {
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
    fn combines_quoted_and_unquoted_parts_of_a_token() {
        assert_eq!(parse("echo hello' world'"), ["echo", "hello world"]);
    }
}
