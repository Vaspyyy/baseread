use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

fn main() {
    let mut arguments = env::args().skip(1);
    let Some(first) = arguments.next() else {
        repl();
        return;
    };

    if first == "--version" {
        println!("BASISREAD 0.2.0");
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

    if first == "--tokens" {
        let Some(path) = arguments.next() else {
            eprintln!("usage: basisread --tokens <file.basis>");
            std::process::exit(2);
        };
        match std::fs::read_to_string(&path).map_err(|error| error.to_string()).and_then(|source| basisread::lex(&source).map_err(|error| error.to_string())) {
            Ok(tokens) => {
                for token in tokens {
                    println!("{}:{} {:?}", token.span.line, token.span.column, token.kind);
                }
            }
            Err(error) => {
                eprintln!("BASISREAD lexer error: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    if first == "build" || first == "compile" {
        let Some(source_path) = arguments.next() else {
            eprintln!("usage: basisread build <file.basis> [-o output]");
            std::process::exit(2);
        };
        let mut output_path = None;
        while let Some(argument) = arguments.next() {
            if argument == "-o" {
                output_path = arguments.next();
            } else {
                eprintln!("unknown build argument `{argument}`");
                std::process::exit(2);
            }
        }
        let output_path = output_path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from(&source_path).with_extension(""));
        match std::fs::read_to_string(&source_path).map_err(|error| error.to_string()).and_then(|source| basisread::compile_source(&source, &output_path, include_str!("lib.rs"), include_str!("lexer.rs"), include_str!("parser.rs"), include_str!("codegen.rs")).map_err(|error| error.to_string())) {
            Ok(()) => println!("built {}", output_path.display()),
            Err(error) => {
                eprintln!("BASISREAD build error: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    if arguments.next().is_some() {
        eprintln!("usage: basisread [--version | --check <file.basis> | --tokens <file.basis> | build <file.basis> [-o output] | <file.basis>]");
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
    println!("BASISREAD 0.2.0");
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
