use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    let input = if args.len() > 1 {
        std::fs::read_to_string(&args[1]).unwrap_or_else(|e| {
            eprintln!("Error reading file '{}': {}", args[1], e);
            std::process::exit(1);
        })
    } else {
        let mut buf = String::new();
        std::io::stdin()
            .read_to_string(&mut buf)
            .unwrap_or_else(|e| {
                eprintln!("Error reading stdin: {}", e);
                std::process::exit(1);
            });
        buf
    };

    let result = rsusfm3::builder::parse(&input);

    // Print diagnostics
    for diag in result.diagnostics.iter() {
        eprintln!("[{}:{}] {}", diag.span.start, diag.span.end, diag);
    }

    // Run validation
    let validation_diags = rsusfm3::validation::validate(&result.document);
    for diag in validation_diags.iter() {
        eprintln!("[{}:{}] {}", diag.span.start, diag.span.end, diag);
    }

    // Determine output format from args
    let format = args.get(2).map(|s| s.as_str()).unwrap_or("usj");

    match format {
        "usj" => {
            let json = rsusfm3::usj::to_usj_string_pretty(&result.document)
                .expect("USJ serialization failed");
            println!("{json}");
        }
        "usx" => {
            let xml =
                rsusfm3::usx::to_usx_string(&result.document).expect("USX serialization failed");
            println!("{xml}");
        }
        "usfm" => {
            let usfm = rsusfm3::usfm::to_usfm_string(&result.document);
            print!("{usfm}");
        }
        "vref" => {
            let json = rsusfm3::vref::to_vref_json_string(&result.document);
            println!("{json}");
        }
        other => {
            eprintln!("Unknown format '{other}'. Use 'usj', 'usx', 'usfm', or 'vref'.");
            std::process::exit(1);
        }
    }
}
