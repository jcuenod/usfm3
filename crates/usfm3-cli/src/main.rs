use std::io::Read;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, inline_spans, input_path) = parse_args(&args);
    let input = read_input(input_path.as_deref());

    match command.as_str() {
        "tokens" => print_json(&usfm3::tokenize(&input)),
        "cst" => {
            let cst = usfm3::parse_cst(&input);
            print_json(&usfm3::cst::export(&cst));
        }
        "ast" => {
            let parsed = usfm3::parse_ast(&input, usfm3::ParseOptions::default());
            print_json(&parsed.ast);
        }
        "diagnostics" => {
            let parsed = usfm3::parse_ast(&input, usfm3::ParseOptions { diagnostics: true });
            print_json(&parsed.diagnostics.unwrap_or_default());
        }
        "usj" => {
            let parsed = usfm3::parse(&input, usfm3::ParseOptions::default());
            let json = parsed
                .to_usj(usfm3::usj::UsjOptions {
                    include_spans: inline_spans,
                })
                .unwrap_or_else(|error| exit_with(&format!("USJ serialization failed: {error}")));
            println!(
                "{}",
                serde_json::to_string_pretty(&json).unwrap_or_else(|error| exit_with(&format!(
                    "JSON serialization failed: {error}"
                )))
            );
        }
        "usx" => {
            let parsed = usfm3::parse(&input, usfm3::ParseOptions::default());
            println!(
                "{}",
                parsed.to_usx().unwrap_or_else(|error| exit_with(&format!(
                    "USX serialization failed: {error}"
                )))
            );
        }
        "usfm" => {
            let parsed = usfm3::parse(&input, usfm3::ParseOptions::default());
            print!("{}", parsed.to_usfm());
        }
        "vref" => {
            let parsed = usfm3::parse(&input, usfm3::ParseOptions::default());
            print_json(&parsed.to_vref());
        }
        other => exit_with(&format!(
            "Unknown subcommand '{other}'. Use one of: tokens, cst, ast, diagnostics, usj, usx, usfm, vref."
        )),
    }
}

fn parse_args(args: &[String]) -> (String, bool, Option<String>) {
    if args.is_empty() {
        print_usage_and_exit();
    }

    let command = args[0].clone();
    let mut inline_spans = false;
    let mut input_path = None;

    for arg in &args[1..] {
        match arg.as_str() {
            "--inline-spans" => inline_spans = true,
            "--help" | "-h" => print_usage_and_exit(),
            value if value.starts_with('-') => {
                exit_with(&format!("Unknown flag '{value}'."));
            }
            value => {
                if input_path.is_some() {
                    exit_with("Expected at most one input path.");
                }
                input_path = Some(value.to_string());
            }
        }
    }

    if inline_spans && command != "usj" {
        exit_with("--inline-spans is only supported for the 'usj' subcommand.");
    }

    (command, inline_spans, input_path)
}

fn read_input(path: Option<&str>) -> String {
    if let Some(path) = path {
        return std::fs::read_to_string(path).unwrap_or_else(|error| {
            exit_with(&format!("Error reading file '{path}': {error}"));
        });
    }

    let mut buffer = String::new();
    std::io::stdin()
        .read_to_string(&mut buffer)
        .unwrap_or_else(|error| exit_with(&format!("Error reading stdin: {error}")));
    buffer
}

fn print_json<T: serde::Serialize>(value: &T) {
    println!(
        "{}",
        serde_json::to_string_pretty(value)
            .unwrap_or_else(|error| exit_with(&format!("JSON serialization failed: {error}")))
    );
}

fn print_usage_and_exit() -> ! {
    exit_with(
        "Usage: usfm3 <tokens|cst|ast|diagnostics|usj|usx|usfm|vref> [input-path] [--inline-spans]",
    )
}

fn exit_with(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}
