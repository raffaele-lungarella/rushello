use std::io::{self, Write};
use std::str::FromStr;

fn main() {
    repl();
}

enum Builtin {
    Exit,
}

impl FromStr for Builtin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exit" => Ok(Builtin::Exit),
            cmd => Err(cmd.to_owned()),
        }
    }
}

fn repl() {
    loop {
        print!("$ ");
        io::stdout().flush().unwrap();

        let mut command = String::new();
        if io::stdin().read_line(&mut command).unwrap() == 0 {
            println!();
            break;
        }

        match Builtin::from_str(command.trim()) {
            Ok(Builtin::Exit) => break,
            Err(cmd) => println!("{cmd}: command not found"),
        }
    }
}
