use clap::Parser;
use js_beautify_rs::cross_version::{AlignConfig, CrossVersionAligner};
use js_beautify_rs::tokenizer::Tokenizer;
use js_beautify_rs::webpack_module_extractor::ModuleExtractor;
use js_beautify_rs::{Options, beautify};
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;

/// A fast JavaScript beautifier and deobfuscator powered by oxc.
///
/// Beautifies minified JavaScript, deobfuscates obfuscated webpack bundles,
/// and extracts webpack modules into separate files.
#[allow(clippy::struct_excessive_bools)]
#[derive(Parser, Debug)]
#[command(name = "jsbeautify", version, about, long_about = None)]
struct Cli {
    /// Input JavaScript file (use "-" for stdin)
    #[arg(value_name = "FILE")]
    input: String,

    /// Write output to a file instead of stdout
    #[arg(short, long, value_name = "FILE")]
    output: Option<String>,

    /// Enable AST-based deobfuscation (19-phase pipeline)
    #[arg(short, long)]
    deobfuscate: bool,

    /// Split webpack chunks into separate files
    #[arg(long)]
    split_chunks: bool,

    /// Directory for chunk output [default: ./chunks]
    #[arg(long, value_name = "DIR")]
    chunk_dir: Option<PathBuf>,

    /// Write chunk metadata to a JSON file
    #[arg(long, value_name = "FILE")]
    chunk_map: Option<PathBuf>,

    /// Extract webpack modules to separate files
    #[arg(long)]
    extract_modules: bool,

    /// Directory for module output [default: ./modules]
    #[arg(long, value_name = "DIR")]
    module_dir: Option<PathBuf>,

    /// Generate a dependency graph in DOT format
    #[arg(long, value_name = "FILE")]
    dependency_graph: Option<PathBuf>,

    /// Generate source maps
    #[arg(long)]
    source_maps: bool,

    /// Indentation size in spaces [default: 4]
    #[arg(long, value_name = "N")]
    indent_size: Option<usize>,

    /// Use tabs for indentation instead of spaces
    #[arg(long)]
    indent_with_tabs: bool,

    /// Path to sourcemap for extracting original variable names
    #[arg(long, value_name = "FILE")]
    sourcemap: Option<PathBuf>,

    /// Second bundle to align with (produces stable diffs)
    #[arg(long, value_name = "FILE")]
    align_with: Option<PathBuf>,

    /// Output path for the aligned second bundle
    #[arg(long, value_name = "FILE")]
    align_output: Option<PathBuf>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let code = if cli.input == "-" {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        buffer
    } else {
        fs::read_to_string(&cli.input)?
    };

    let mut options = Options {
        deobfuscate: cli.deobfuscate,
        split_chunks: cli.split_chunks,
        extract_modules: cli.extract_modules,
        generate_source_map: cli.source_maps,
        indent_with_tabs: cli.indent_with_tabs,
        ..Options::default()
    };

    if let Some(dir) = cli.chunk_dir {
        options.chunk_dir = dir;
    }
    if let Some(path) = cli.chunk_map {
        options.chunk_map_output = Some(path);
    }
    if let Some(dir) = cli.module_dir {
        options.module_dir = dir;
    }
    if let Some(path) = &cli.dependency_graph {
        options.dependency_graph = Some(path.clone());
    }
    if let Some(size) = cli.indent_size {
        options.indent_size = size;
    }

    if options.extract_modules {
        eprintln!("[WEBPACK] Extracting modules...");
        let mut tokenizer = Tokenizer::new(&code);
        let tokens = tokenizer.tokenize()?;
        let mut extractor = ModuleExtractor::new();
        extractor.extract_modules(&tokens)?;
        extractor.extract_dependencies(&tokens)?;

        eprintln!("[WEBPACK] Found {} modules", extractor.module_count());

        extractor.write_modules(&tokens, &options.module_dir)?;
        eprintln!("[WEBPACK] Modules written to {}", options.module_dir.display());

        if let Some(graph_path) = &options.dependency_graph {
            extractor.generate_dependency_graph(graph_path)?;
            eprintln!("[WEBPACK] Dependency graph written to {}", graph_path.display());
        }

        return Ok(());
    }

    if cli.align_with.is_some() || cli.sourcemap.is_some() {
        options.skip_annotations = true;
        return run_cross_version_align(
            cli.sourcemap.as_ref(),
            cli.align_with.as_ref(),
            cli.align_output.as_ref(),
            cli.output.as_ref(),
            &code,
            &options,
        );
    }

    let beautified = beautify(&code, &options)?;

    if let Some(output_path) = &cli.output {
        fs::write(output_path, &beautified)?;
        eprintln!("Beautified code written to {output_path}");
    } else {
        println!("{beautified}");
    }

    Ok(())
}

fn run_cross_version_align(
    sourcemap_path: Option<&PathBuf>,
    align_with_path: Option<&PathBuf>,
    align_output_path: Option<&PathBuf>,
    output_path: Option<&String>,
    source_code: &str,
    options: &Options,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = AlignConfig {
        sourcemap_path: sourcemap_path.map(|p| p.to_string_lossy().to_string()),
        align_with: align_with_path.map(|p| p.to_string_lossy().to_string()),
        hash_depth: 12,
    };

    let mut aligner = CrossVersionAligner::new(config);

    if let Some(smap_path) = sourcemap_path {
        eprintln!("[ALIGN] Loading sourcemap: {}", smap_path.display());
        let sourcemap_json = fs::read_to_string(smap_path)?;
        let stable_count = aligner.load_sourcemap(&sourcemap_json, source_code)?;
        eprintln!("[ALIGN] Loaded {} stable name mappings", stable_count);
    }

    eprintln!("[ALIGN] Beautifying source...");
    let source_beautified = beautify(source_code, options)?;
    eprintln!("[ALIGN] Source beautified: {} lines", source_beautified.lines().count());

    if let Some(target_path) = align_with_path {
        eprintln!("[ALIGN] Loading target bundle: {}", target_path.display());
        let target_code = fs::read_to_string(target_path)?;

        eprintln!("[ALIGN] Beautifying target...");
        let target_beautified = beautify(&target_code, options)?;
        eprintln!("[ALIGN] Target beautified: {} lines", target_beautified.lines().count());

        eprintln!("[ALIGN] Aligning statements between versions...");
        let (aligned_source, aligned_target, stats) = aligner.align_sources(&source_beautified, &target_beautified);

        eprintln!("[ALIGN] === Results ===");
        eprintln!(
            "[ALIGN] Matched: {} / {} ({:.1}%)",
            stats.matched_statements,
            stats.source_statements,
            stats.match_rate()
        );
        eprintln!("[ALIGN] Source replacements: {}", stats.source_replacements);
        eprintln!("[ALIGN] Target replacements: {}", stats.target_replacements);
        eprintln!("[ALIGN] Canonical names generated: {}", stats.canonical_names_generated);

        if let Some(align_out) = align_output_path {
            fs::write(align_out, &aligned_target)?;
            eprintln!("[ALIGN] Target output written to {}", align_out.display());
        }

        if let Some(out_path) = output_path {
            fs::write(out_path, &aligned_source)?;
            eprintln!("[ALIGN] Source output written to {out_path}");
        } else {
            println!("{aligned_source}");
        }
    } else {
        if let Some(out_path) = output_path {
            fs::write(out_path, &source_beautified)?;
            eprintln!("[ALIGN] Source output written to {out_path}");
        } else {
            println!("{source_beautified}");
        }
    }

    Ok(())
}
