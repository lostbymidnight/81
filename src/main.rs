mod parse;
mod vm;

use std::io::{self, BufWriter};
use std::process::exit;
use std::{env, fs};

const USAGE: &str = concat!(
    "81 ",
    env!("CARGO_PKG_VERSION"),
    ", an interpreter for the 81 esolang (https://esolangs.org/wiki/81)

usage:
  81 <file.81>    run a program from a file
  81 -c <code>    run code given on the command line, one command per line
  81 -h           show this help"
);

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    let src = match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => {
            println!("{USAGE}");
            exit(0);
        }
        [flag, code] if flag == "-c" => code.clone(),
        [path] => fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("error: cannot read {path}: {e}");
            exit(2);
        }),
        _ => {
            eprintln!("{USAGE}");
            exit(2);
        }
    };

    let program = parse::parse(&src).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        exit(1);
    });

    let mut vm = vm::Vm::new(io::stdin().lock(), BufWriter::new(io::stdout().lock()));
    if let Err(e) = vm.run(&program) {
        eprintln!("error: {e}");
        exit(1);
    }
}
