use std::env;

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: basisread <file.basis>");
        std::process::exit(2);
    };

    match basisread::run_file(path) {
        Ok(lines) => {
            for line in lines { println!("{line}"); }
        }
        Err(error) => {
            eprintln!("BASISREAD error: {error}");
            std::process::exit(1);
        }
    }
}
