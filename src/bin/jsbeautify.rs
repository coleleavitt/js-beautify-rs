use js_beautify_rs::tokenizer::Tokenizer;
use js_beautify_rs::webpack_module_extractor::ModuleExtractor;
use js_beautify_rs::{Options, beautify};
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage(&args[0]);
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

    let options = parse_options(&args[1..])?;

    if options.extract_modules {
        eprintln!("[WEBPACK] Extracting modules...");
        let mut tokenizer = Tokenizer::new(&code);
        let tokens = tokenizer.tokenize()?;
        let mut extractor = ModuleExtractor::new();
        extractor.extract_modules(&tokens)?;
        extractor.extract_dependencies(&tokens)?;

        eprintln!("[WEBPACK] Found {} modules", extractor.module_count());

        extractor.write_modules(&tokens, &options.module_dir)?;
        eprintln!(
            "[WEBPACK] Modules written to {}",
            options.module_dir.display()
        );

        if let Some(graph_path) = &options.dependency_graph {
            extractor.generate_dependency_graph(graph_path)?;
            eprintln!(
                "[WEBPACK] Dependency graph written to {}",
                graph_path.display()
            );
        }

        return Ok(());
    }

    let beautified = beautify(&code, &options)?;

    if let Some(output_path) = get_output_path(&args) {
        fs::write(&output_path, beautified)?;
        eprintln!("Beautified code written to {}", output_path);
    } else {
        println!("{}", beautified);
    }

    Ok(())
}

fn print_usage(program: &str) {
    eprintln!("Usage: {} <input_file> [OPTIONS]", program);
    eprintln!("   or: cat file.js | {} > output.js", program);
    eprintln!();
    eprintln!("OPTIONS:");
    eprintln!("  -o, --output <file>          Write output to file");
    eprintln!("  --deobfuscate                Enable deobfuscation");
    eprintln!("  --split-chunks               Split webpack chunks into separate files");
    eprintln!("  --chunk-dir <dir>            Directory for chunk output (default: ./chunks)");
    eprintln!("  --chunk-map <file>           Write chunk metadata to JSON file");
    eprintln!("  --extract-modules            Extract webpack modules to separate files");
    eprintln!("  --module-dir <dir>           Directory for module output (default: ./modules)");
    eprintln!("  --dependency-graph <file>    Generate dependency graph (DOT format)");
    eprintln!("  --source-maps                Generate source maps");
}

fn parse_options(args: &[String]) -> Result<Options, Box<dyn std::error::Error>> {
    let mut options = Options::default();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--deobfuscate" => {
                options.deobfuscate = true;
                i = i.checked_add(1).ok_or("index overflow")?;
            }
            "--split-chunks" => {
                options.split_chunks = true;
                i = i.checked_add(1).ok_or("index overflow")?;
            }
            "--chunk-dir" => {
                if i.checked_add(1).ok_or("index overflow")? >= args.len() {
                    return Err("--chunk-dir requires a value".into());
                }
                i = i.checked_add(1).ok_or("index overflow")?;
                options.chunk_dir = PathBuf::from(&args[i]);
                i = i.checked_add(1).ok_or("index overflow")?;
            }
            "--chunk-map" => {
                if i.checked_add(1).ok_or("index overflow")? >= args.len() {
                    return Err("--chunk-map requires a value".into());
                }
                i = i.checked_add(1).ok_or("index overflow")?;
                options.chunk_map_output = Some(PathBuf::from(&args[i]));
                i = i.checked_add(1).ok_or("index overflow")?;
            }
            "--extract-modules" => {
                options.extract_modules = true;
                i = i.checked_add(1).ok_or("index overflow")?;
            }
            "--module-dir" => {
                if i.checked_add(1).ok_or("index overflow")? >= args.len() {
                    return Err("--module-dir requires a value".into());
                }
                i = i.checked_add(1).ok_or("index overflow")?;
                options.module_dir = PathBuf::from(&args[i]);
                i = i.checked_add(1).ok_or("index overflow")?;
            }
            "--dependency-graph" => {
                if i.checked_add(1).ok_or("index overflow")? >= args.len() {
                    return Err("--dependency-graph requires a value".into());
                }
                i = i.checked_add(1).ok_or("index overflow")?;
                options.dependency_graph = Some(PathBuf::from(&args[i]));
                i = i.checked_add(1).ok_or("index overflow")?;
            }
            "--source-maps" => {
                options.generate_source_map = true;
                i = i.checked_add(1).ok_or("index overflow")?;
            }
            "-o" | "--output" => {
                i = i.checked_add(2).ok_or("index overflow")?;
            }
            _ => {
                i = i.checked_add(1).ok_or("index overflow")?;
            }
        }
    }

    Ok(options)
}

fn get_output_path(args: &[String]) -> Option<String> {
    for i in 1..args.len() {
        if (args[i] == "-o" || args[i] == "--output") && i.checked_add(1)? < args.len() {
            return Some(args[i.checked_add(1)?].clone());
        }
    }
    None
}
