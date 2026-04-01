use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let no_validate = args.iter().any(|a| a == "--no-validate");
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| a.as_str() != "--no-validate")
        .collect();

    let input = if let Some(path) = positional.first() {
        std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Error reading file '{}': {}", path, e);
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

    let result = usfm3::parse_full(
        &input,
        usfm3::ParseOptions {
            validate: !no_validate,
        },
    );

    if !no_validate {
        // Print parser diagnostics
        for diag in result.parser_diagnostics.iter() {
            eprintln!("[{}:{}] {}", diag.span.start, diag.span.end, diag);
        }

        for diag in result.validation_diagnostics.iter() {
            eprintln!("[{}:{}] {}", diag.span.start, diag.span.end, diag);
        }
    }

    // Determine output format from args
    let format = positional.get(1).map(|s| s.as_str()).unwrap_or("usj");

    match format {
        "usj" => {
            let json =
                usfm3::usj::to_usj_string_pretty(&result.ast).expect("USJ serialization failed");
            println!("{json}");
        }
        "usx" => {
            let xml = usfm3::usx::to_usx_string(&result.ast).expect("USX serialization failed");
            println!("{xml}");
        }
        "usfm" => {
            let usfm = usfm3::usfm::to_usfm_string(&result.ast);
            print!("{usfm}");
        }
        "vref" => {
            let json = usfm3::vref::to_vref_json_string(&result.ast);
            println!("{json}");
        }
        other => {
            eprintln!("Unknown format '{other}'. Use 'usj', 'usx', 'usfm', or 'vref'.");
            std::process::exit(1);
        }
    }
}
