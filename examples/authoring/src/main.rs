//! CPU-only inspection of an external, bounded mathematical structure.

use std::{path::Path, time::Instant};
use world_authoring::compile_file;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: authoring-inspect FILE.json [--measure]")?;
    let measure = match args.next() {
        None => false,
        Some(flag) if flag == "--measure" => true,
        _ => return Err("usage: authoring-inspect FILE.json [--measure]".into()),
    };
    if args.next().is_some() {
        return Err("usage: authoring-inspect FILE.json [--measure]".into());
    }
    let started = Instant::now();
    let structure = compile_file(Path::new(&path))?;
    let mut summary = serde_json::json!({
        "kind": "authored-structure",
        "id": structure.id(),
        "revision": structure.revision(),
        "input_bytes": structure.source().len(),
        "expanded_symbols": structure.expanded_symbols(),
        "segments": structure.segments().len(),
        "max_stack": structure.max_stack_used(),
        "work": structure.work()
    });
    if measure {
        summary["load_compile_ms"] = serde_json::json!(started.elapsed().as_secs_f64() * 1000.0);
    }
    println!("{summary}");
    Ok(())
}
