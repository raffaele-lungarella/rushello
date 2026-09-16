use std::io::{self, Write};
use std::str::FromStr;

fn main() {
    repl();
}

enum Builtin {
    Exit,
    Echo,
}

impl FromStr for Builtin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exit" => Ok(Builtin::Exit),
            "echo" => Ok(Builtin::Echo),
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

        match Builtin::from_str(command) {
            Ok(Builtin::Exit) => break,
            Ok(Builtin::Echo) => println!("{}", tokens.collect::<Vec<_>>().join(" ")),
            Err(cmd) => println!("{cmd}: command not found"),
        }
    }
}
