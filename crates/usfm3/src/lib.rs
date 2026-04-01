pub mod ast;
pub mod builder;
pub mod cst;
pub mod diagnostics;
pub mod lexer;
pub mod markers;
pub mod usfm;
pub mod usj;
pub mod usx;
pub mod validation;
pub mod vref;

#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    pub validate: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self { validate: true }
    }
}

#[derive(Debug, Clone)]
pub struct ParseArtifacts {
    pub cst: cst::CstDocument,
    pub ast: ast::Document,
    pub parser_diagnostics: diagnostics::DiagnosticList,
    pub validation_diagnostics: diagnostics::DiagnosticList,
}

pub fn parse_owned(input: String) -> builder::LowerResult {
    builder::parse_owned(input)
}

pub fn parse_full(input: &str, options: ParseOptions) -> ParseArtifacts {
    parse_full_owned(input.to_string(), options)
}

pub fn parse_full_owned(input: String, options: ParseOptions) -> ParseArtifacts {
    let cst = cst::parse_owned(input);
    let lowered = builder::lower(&cst);
    let validation_diagnostics = if options.validate {
        validation::validate(&lowered.ast)
    } else {
        diagnostics::DiagnosticList::new()
    };
    ParseArtifacts {
        cst,
        ast: lowered.ast,
        parser_diagnostics: lowered.diagnostics,
        validation_diagnostics,
    }
}
