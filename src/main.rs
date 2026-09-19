mod parser;
mod trie;

use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::FromStr;

use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, Editor, Helper};

use crate::trie::Trie;

fn main() {
    repl();
}

fn repl() {
    let config = Config::builder()
        .completion_type(CompletionType::Circular)
        .build();
    let mut editor = Editor::<ShellHelper, DefaultHistory>::with_config(config)
        .expect("failed to initialize editor");
    editor.set_helper(Some(ShellHelper::new()));
    loop {
        let input = match editor.readline("$ ") {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => continue,
            Err(ReadlineError::Eof) => break,
            Err(error) => {
                eprintln!("readline: {error}");
                break;
            }
        };

        if input.is_empty() {
            continue; // Noop
        }

        let parsed = match parser::parse(&input) {
            Ok(parsed) => parsed,
            Err(error) => {
                eprintln!("parse: {error}");
                continue;
            }
        };

        match execute(&parsed) {
            Ok(true) => break,
            Ok(false) => {}
            Err(error) => eprintln!("{error}"),
        }
    }
}

struct ShellHelper {
    builtins: Trie,
}

impl ShellHelper {
    fn new() -> Self {
        let mut builtins = Trie::default();
        for builtin in Builtin::ALL {
            builtins.insert(builtin.name());
        }
        Self { builtins }
    }
}

impl Completer for ShellHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before_cursor = &line[..pos];
        let prefix = before_cursor.trim_start();
        let start = before_cursor.len() - prefix.len();

        // Only complete a bare command name, not arguments or shell syntax.
        if prefix
            .chars()
            .any(|ch| ch.is_whitespace() || "\\\"'\\\\<>|;&".contains(ch))
        {
            return Ok((pos, Vec::new()));
        }

        let mut matches = self.builtins.words_with_prefix(prefix);
        matches.sort();
        let append_space = matches.len() == 1 && pos == line.len();
        let candidates = matches
            .into_iter()
            .map(|name| Pair {
                replacement: if append_space {
                    format!("{name} ")
                } else {
                    name.clone()
                },
                display: name,
            })
            .collect();

        Ok((start, candidates))
    }
}

impl Helper for ShellHelper {}
impl Highlighter for ShellHelper {}
impl Validator for ShellHelper {}
impl Hinter for ShellHelper {
    type Hint = String;
}

fn execute(parsed: &parser::ParsedCommand) -> io::Result<bool> {
    let mut output_file = None;
    let mut error_file: Option<fs::File> = None;
    let mut stderr = io::stderr();
    // Open targets in order; the last destination for each stream wins.
    for redirection in &parsed.redirects {
        let target = &redirection.target;
        let file = match fs::OpenOptions::new()
            .write(true)
            .create(true)
            .append(redirection.append)
            .truncate(!redirection.append)
            .open(target)
        {
            Ok(file) => file,
            Err(error) => {
                let errors: &mut dyn Write = match error_file.as_mut() {
                    Some(file) => file,
                    None => &mut stderr,
                };
                writeln!(errors, "{target}: {error}")?;
                return Ok(false);
            }
        };
        match redirection.stream {
            parser::OutputStream::Stdout => output_file = Some(file),
            parser::OutputStream::Stderr => error_file = Some(file),
        }
    }

    let Some((command, args)) = parsed.args.split_first() else {
        return Ok(false);
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    let builtin = match Builtin::from_str(command) {
        Ok(builtin) => builtin,
        Err(_) => {
            run_executable(command, &args, output_file, error_file)?;
            return Ok(false);
        }
    };

    let mut stdout = io::stdout();
    let mut context = ExecutionContext {
        stdout: match output_file.as_mut() {
            Some(file) => file,
            None => &mut stdout,
        },
        stderr: match error_file.as_mut() {
            Some(file) => file,
            None => &mut stderr,
        },
    };

    let result = match builtin {
        Builtin::Exit => return Ok(true),
        Builtin::Echo => context.echo(&args),
        Builtin::Type => context.run_type(&args),
        Builtin::Pwd => context.pwd(),
        Builtin::Cd => context.cd(&args),
    };
    if let Err(error) = result.and_then(|_| context.stdout.flush()) {
        writeln!(context.stderr, "{command}: {error}")?;
    }
    context.stderr.flush()?;
    Ok(false)
}

enum Builtin {
    Exit,
    Echo,
    Type,
    Pwd,
    Cd,
}

impl Builtin {
    const ALL: [Self; 5] = [Self::Exit, Self::Echo, Self::Type, Self::Pwd, Self::Cd];

    fn name(&self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Echo => "echo",
            Self::Type => "type",
            Self::Pwd => "pwd",
            Self::Cd => "cd",
        }
    }
}

impl FromStr for Builtin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exit" => Ok(Builtin::Exit),
            "echo" => Ok(Builtin::Echo),
            "type" => Ok(Builtin::Type),
            "pwd" => Ok(Builtin::Pwd),
            "cd" => Ok(Builtin::Cd),
            cmd => Err(cmd.to_owned()),
        }
    }
}

struct ExecutionContext<'a> {
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
}

impl ExecutionContext<'_> {
    fn echo(&mut self, args: &[&str]) -> io::Result<()> {
        writeln!(self.stdout, "{}", args.join(" "))
    }

    fn pwd(&mut self) -> io::Result<()> {
        match env::current_dir() {
            Ok(directory) => writeln!(self.stdout, "{}", directory.display()),
            Err(error) => writeln!(self.stderr, "pwd: {error}"),
        }
    }

    fn cd(&mut self, args: &[&str]) -> io::Result<()> {
        let target = match args.first() {
            Some(&"~") | None => match env::var_os("HOME") {
                Some(home) => PathBuf::from(home),
                None => {
                    return writeln!(self.stderr, "cd: HOME not set");
                }
            },
            Some(path) => PathBuf::from(path),
        };

        if let Err(error) = env::set_current_dir(&target) {
            let message = error.to_string();
            let message = message.split(" (os error").next().unwrap_or(&message);
            writeln!(self.stderr, "cd: {}: {message}", target.display())?;
        }
        Ok(())
    }

    fn run_type(&mut self, args: &[&str]) -> io::Result<()> {
        for arg in args {
            if Builtin::ALL.iter().any(|builtin| builtin.name() == *arg) {
                writeln!(self.stdout, "{arg} is a shell builtin")?;
                continue;
            }

            match find_executable(arg) {
                Some(path) => writeln!(self.stdout, "{arg} is {}", path.display())?,
                None => writeln!(self.stdout, "{arg}: not found")?,
            }
        }
        Ok(())
    }
}

fn find_executable(command: &str) -> Option<PathBuf> {
    env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join(command))
        .find(|candidate| {
            fs::metadata(candidate).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}

fn run_executable(
    command: &str,
    args: &[&str],
    output_file: Option<fs::File>,
    mut error_file: Option<fs::File>,
) -> io::Result<()> {
    let mut stderr = io::stderr();
    let Some(path) = find_executable(command) else {
        let errors: &mut dyn Write = match error_file.as_mut() {
            Some(file) => file,
            None => &mut stderr,
        };
        return writeln!(errors, "{command}: command not found");
    };

    let mut child = Command::new(path);
    child.arg0(command).args(args);
    if let Some(file) = output_file {
        child.stdout(Stdio::from(file));
    }
    if let Some(file) = error_file.as_ref() {
        child.stderr(Stdio::from(file.try_clone()?));
    }
    if let Err(error) = child.status() {
        let errors: &mut dyn Write = match error_file.as_mut() {
            Some(file) => file,
            None => &mut stderr,
        };
        writeln!(errors, "{command}: {error}")?;
    }
    Ok(())
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    fn complete(line: &str, pos: usize) -> (usize, Vec<Pair>) {
        let history = DefaultHistory::new();
        ShellHelper::new()
            .complete(line, pos, &Context::new(&history))
            .unwrap()
    }

    #[test]
    fn completes_builtin_with_trailing_space() {
        let (start, candidates) = complete("ech", 3);
        assert_eq!(start, 0);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, "echo");
        assert_eq!(candidates[0].replacement, "echo ");
    }

    #[test]
    fn returns_sorted_ambiguous_matches_without_spaces() {
        let (_, candidates) = complete("e", 1);
        let replacements: Vec<_> = candidates
            .iter()
            .map(|pair| pair.replacement.as_str())
            .collect();
        assert_eq!(replacements, ["echo", "exit"]);
    }

    #[test]
    fn preserves_leading_whitespace() {
        let (start, candidates) = complete("  pw", 4);
        assert_eq!(start, 2);
        assert_eq!(candidates[0].replacement, "pwd ");
    }

    #[test]
    fn leaves_arguments_and_unknown_commands_alone() {
        for line in ["echo e", "echo ", "unknown", "echo>e", "'ec", "\"ec"] {
            assert!(complete(line, line.len()).1.is_empty(), "{line}");
        }
    }

    #[test]
    fn uses_cursor_position_without_adding_space_before_existing_text() {
        let (start, candidates) = complete("ec argument", 2);
        assert_eq!(start, 0);
        assert_eq!(candidates[0].replacement, "echo");
    }

    #[test]
    fn empty_input_lists_all_builtins() {
        assert_eq!(complete("", 0).1.len(), Builtin::ALL.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_write_to_context_output() {
        let mut output = Vec::new();
        let mut errors = Vec::new();
        let mut context = ExecutionContext {
            stdout: &mut output,
            stderr: &mut errors,
        };
        context.echo(&["hello", "world"]).unwrap();
        context.run_type(&["echo", "pwd"]).unwrap();
        context.pwd().unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!(
                "hello world\necho is a shell builtin\npwd is a shell builtin\n{}\n",
                env::current_dir().unwrap().display()
            )
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn builtin_propagates_output_errors() {
        struct BrokenWriter;
        impl Write for BrokenWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut output = BrokenWriter;
        let mut errors = Vec::new();
        let mut context = ExecutionContext {
            stdout: &mut output,
            stderr: &mut errors,
        };
        assert_eq!(
            context.echo(&["hello"]).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
