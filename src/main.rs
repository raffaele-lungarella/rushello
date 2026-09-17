use std::env;
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
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
            Ok(Builtin::Pwd) => run_pwd(),
            Ok(Builtin::Cd) => run_cd(&args),
            Err(cmd) => run_executable(&cmd, &args),
        }
    }
}

fn run_pwd() {
    match env::current_dir() {
        Ok(directory) => println!("{}", directory.display()),
        Err(error) => eprintln!("pwd: {error}"),
    }
}

fn run_cd(args: &[&str]) {
    let target = match args.first() {
        Some(&"~") | None => match env::var_os("HOME") {
            Some(home) => PathBuf::from(home),
            None => {
                eprintln!("cd: HOME not set");
                return;
            }
        },
        Some(path) => PathBuf::from(path),
    };

    if let Err(error) = env::set_current_dir(&target) {
        let message = error.to_string();
        let message = message.split(" (os error").next().unwrap_or(&message);
        eprintln!("cd: {}: {message}", target.display());
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

    if let Err(error) = Command::new(path).arg0(command).args(args).status() {
        eprintln!("{command}: {error}");
    }
}
