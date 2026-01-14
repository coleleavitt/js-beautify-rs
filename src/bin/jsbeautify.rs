use js_beautify_rs::{Options, beautify};
use std::env;
use std::fs;
use std::io::{self, Read};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!(
            "Usage: {} <input_file> [-o <output_file>] [--deobfuscate]",
            args[0]
        );
        eprintln!("   or: cat file.js | {} > output.js", args[0]);
        std::process::exit(1);
    }

    let input_path = &args[1];
    let code = if input_path == "-" {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    } else {
        fs::read_to_string(input_path)?
    };

    let mut options = Options::default();
    options.deobfuscate = true;

    let beautified = beautify(&code, &options)?;

    if args.len() > 3 && args[2] == "-o" {
        fs::write(&args[3], beautified)?;
        eprintln!("Beautified code written to {}", args[3]);
    } else {
        println!("{}", beautified);
    }

    Ok(())
}
