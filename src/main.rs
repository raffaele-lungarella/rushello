use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::str::FromStr;

fn main() {
    repl();
}

enum Builtin {
    Exit,
    Echo,
    Type,
}

impl Builtin {
    const ALL: [Self; 3] = [Self::Exit, Self::Echo, Self::Type];

    fn name(&self) -> &'static str {
        match self {
            Self::Exit => "exit",
            Self::Echo => "echo",
            Self::Type => "type",
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
            cmd => Err(cmd.to_owned()),
        }
    }
}

fn repl() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).unwrap() == 0 {
            println!();
            break;
        }

        let mut tokens = input.split_whitespace();
        let Some(command) = tokens.next() else {
            continue;
        };

        let args: Vec<&str> = tokens.collect();

        match Builtin::from_str(command) {
            Ok(Builtin::Exit) => break,
            Ok(Builtin::Echo) => println!("{}", args.join(" ")),
            Ok(Builtin::Type) => run_type(&args),
            Err(cmd) => run_executable(&cmd, &args),
        }
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

fn run_type(args: &[&str]) {
    for arg in args {
        if Builtin::ALL.iter().any(|builtin| builtin.name() == *arg) {
            println!("{arg} is a shell builtin");
            continue;
        }

        match find_executable(arg) {
            Some(path) => println!("{arg} is {}", path.display()),
            None => println!("{arg}: not found"),
        }
    }
}

fn run_executable(command: &str, args: &[&str]) {
    let Some(path) = find_executable(command) else {
        println!("{command}: command not found");
        return;
    };

    if let Err(error) = Command::new(path).args(args).status() {
        eprintln!("{command}: {error}");
    }
}
