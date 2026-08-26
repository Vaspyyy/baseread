use std::env;
use std::io::{self, BufRead, Write};

fn main() {
    let mut arguments = env::args().skip(1);
    let Some(first) = arguments.next() else {
        repl();
        return;
    };

    if first == "--version" {
        println!("BASISREAD 0.1.0");
        return;
    }

    if first == "--check" {
        let Some(path) = arguments.next() else {
            eprintln!("usage: basisread --check <file.basis>");
            std::process::exit(2);
        };
        match std::fs::read_to_string(&path).map_err(|error| error.to_string()).and_then(|source| basisread::parse(&source).map(|_| ()).map_err(|error| error.to_string())) {
            Ok(()) => println!("ok: {path}"),
            Err(error) => {
                eprintln!("BASISREAD error: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    if arguments.next().is_some() {
        eprintln!("usage: basisread [--version | --check <file.basis> | <file.basis>]");
        std::process::exit(2);
    }

    match basisread::run_file(first) {
        Ok(lines) => {
            for line in lines { println!("{line}"); }
        }
        Err(error) => {
            eprintln!("BASISREAD error: {error}");
            std::process::exit(1);
        }
    }
}

fn repl() {
    println!("BASISREAD 0.1.0");
    println!("Type `exit` to leave.");
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut source = String::new();
    loop {
        print!("basisread> ");
        let _ = io::stdout().flush();
        let mut line = String::new();
        if input.read_line(&mut line).unwrap_or(0) == 0 { break; }
        if source.is_empty() && line.trim() == "exit" { break; }
        source.push_str(&line);
        if block_depth(&source) != 0 { continue; }
        match basisread::parse(&source).and_then(|program| basisread::run(&program)) {
            Ok(lines) => for line in lines { println!("{line}"); },
            Err(error) => eprintln!("BASISREAD error: {error}"),
        }
        source.clear();
    }
}

fn block_depth(source: &str) -> i32 {
    source.lines().fold(0, |depth, line| {
        let line = line.trim();
        let opens = ["define ", "when ", "repeat ", "while ", "for each "].iter().any(|prefix| line.starts_with(prefix));
        depth + if opens { 1 } else { 0 } - if line == "end" { 1 } else { 0 }
    })
}
