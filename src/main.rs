use std::io::{self, Write};
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
            Ok(Builtin::Type) => {
                if let Some(arg) = args.first() {
                    if Builtin::ALL.iter().any(|builtin| builtin.name() == *arg) {
                        println!("{arg} is a shell builtin");
                    } else {
                        println!("{arg}: not found");
                    }
                }
            }
            Err(cmd) => println!("{cmd}: command not found"),
        }
    }
}
