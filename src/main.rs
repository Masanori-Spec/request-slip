use std::{env, fs::File, io::{self, Read, Write}, process::ExitCode};

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args == ["--help"] || args.is_empty() {
        println!("RequestSlip 0.1 — static Hurl-to-cURL text, no requests executed\nUsage: request-slip FILE.hurl [--select 1,3] [--format json|text]\nReads only FILE.hurl (or - for stdin). Writes review/output to stdout.\nRequires a supported literal subset; rejects auth, options, cookies and dynamic forms.\nJSON includes argv plus review fields; text contains POSIX-shell-quoted commands.\nReview the output before running anything yourself. See README for limits.");
        return Ok(());
    }
    let source_path = &args[0];
    let mut selection = None;
    let mut format = "json";
    let mut format_seen = false;
    let mut i = 1;
    while i < args.len() {
        if i + 1 >= args.len() { return Err("A flag is missing its value".into()); }
        match args[i].as_str() {
            "--select" if selection.is_none() => selection = Some(args[i + 1].as_str()),
            "--format" if !format_seen => { format = &args[i + 1]; format_seen = true; }
            _ => return Err("Unknown or repeated argument".into()),
        }
        i += 2;
    }
    if !["json", "text"].contains(&format) { return Err("Format must be json or text".into()); }
    let mut bytes = Vec::new();
    let reader: Box<dyn Read> = if source_path == "-" { Box::new(io::stdin()) } else {
        Box::new(File::open(source_path).map_err(|_| "Cannot open the selected file")?)
    };
    reader.take((request_slip::MAX_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|_| "Cannot read the selected input")?;
    let source = std::str::from_utf8(&bytes).map_err(|_| "Input must be valid UTF-8")?;
    let receipt = request_slip::convert(source, selection)?;
    let output = if format == "json" {
        serde_json::to_string_pretty(&receipt).map_err(|_| "Cannot serialize the receipt")? + "\n"
    } else { request_slip::text_export(&receipt) };
    if output.len() > 1_048_576 { return Err("Serialized output exceeds 1 MiB".into()); }
    io::stdout().lock().write_all(output.as_bytes()).map_err(|_| "Cannot write output")?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => { eprintln!("RequestSlip: {message}"); ExitCode::from(2) }
    }
}
