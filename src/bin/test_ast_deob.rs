use js_beautify_rs::ast_deobfuscate::AstDeobfuscator;
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <js-file>", args[0]);
        std::process::exit(1);
    }

    let path = &args[1];
    let code = fs::read_to_string(path).expect("Failed to read file");

    eprintln!("Testing AST deobfuscator on: {}", path);
    eprintln!("File size: {} bytes", code.len());

    let mut deobfuscator = AstDeobfuscator::new();
    match deobfuscator.deobfuscate(&code) {
        Ok(output) => {
            eprintln!("SUCCESS: Deobfuscation complete");
            eprintln!("Output size: {} bytes", output.len());
            println!("{}", output);
        }
        Err(e) => {
            eprintln!("FAILED: {:?}", e);
            std::process::exit(1);
        }
    }
}
