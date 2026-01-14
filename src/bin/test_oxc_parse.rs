use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_span::SourceType;
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
    
    println!("Testing Oxc parser on: {}", path);
    println!("File size: {} bytes", code.len());
    
    let allocator = Allocator::default();
    let source_type = SourceType::mjs();
    let result = Parser::new(&allocator, &code, source_type).parse();
    
    if result.errors.is_empty() {
        println!("✅ SUCCESS: Oxc parsed the file successfully!");
        println!("   - Program has {} statements", result.program.body.len());
    } else {
        println!("❌ FAILED: Oxc could not parse the file");
        println!("   - Error count: {}", result.errors.len());
        println!("\nFirst 5 errors:");
        for (i, error) in result.errors.iter().take(5).enumerate() {
            println!("   {}. {:?}", i + 1, error);
        }
    }
}
