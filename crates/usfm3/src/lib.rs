use std::cell::OnceCell;
use serde::Serialize;

pub mod ast;
pub mod builder;
pub mod cst;
pub mod diagnostics;
pub mod lexer;
pub mod markers;
pub mod source_map;
pub mod usfm;
pub mod usj;
pub mod usx;
pub mod validation;
pub mod vref;

#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    pub diagnostics: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self { diagnostics: false }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AstDocument {
    pub ast: ast::Document,
    pub source_map: source_map::SourceMap,
    pub diagnostics: Option<Vec<diagnostics::Diagnostic>>,
}

pub struct ParsedDocument {
    source: String,
    options: ParseOptions,
    tokens: OnceCell<Vec<lexer::TokenSpan>>,
    cst: OnceCell<cst::CstDocument>,
    ast_document: OnceCell<AstDocument>,
}

impl ParsedDocument {
    fn new(source: String, options: ParseOptions) -> Self {
        Self {
            source,
            options,
            tokens: OnceCell::new(),
            cst: OnceCell::new(),
            ast_document: OnceCell::new(),
        }
    }

    pub fn tokens(&self) -> &[lexer::TokenSpan] {
        self.tokens
            .get_or_init(|| lexer::exported_tokens(&self.source))
            .as_slice()
    }

    pub fn cst(&self) -> &cst::CstDocument {
        self.cst.get_or_init(|| cst::parse(&self.source))
    }

    pub fn ast_document(&self) -> &AstDocument {
        self.ast_document
            .get_or_init(|| builder::lower(self.cst(), self.options))
    }

    pub fn ast(&self) -> &ast::Document {
        &self.ast_document().ast
    }

    pub fn source_map(&self) -> &source_map::SourceMap {
        &self.ast_document().source_map
    }

    pub fn diagnostics(&self) -> Option<&[diagnostics::Diagnostic]> {
        self.ast_document().diagnostics.as_deref()
    }

    pub fn to_usj(&self, options: usj::UsjOptions) -> Result<serde_json::Value, usj::UsjError> {
        usj::to_usj_value_with_options(self.ast(), Some(self.source_map()), options)
    }

    pub fn to_usx(&self) -> Result<String, usx::UsxError> {
        usx::to_usx_string(self.ast())
    }

    pub fn to_usfm(&self) -> String {
        usfm::to_usfm_string(self.ast())
    }

    pub fn to_vref(&self) -> serde_json::Map<String, serde_json::Value> {
        vref::to_vref_map(self.ast())
    }
}

pub fn parse(input: &str, options: ParseOptions) -> ParsedDocument {
    ParsedDocument::new(input.to_string(), options)
}

pub fn parse_owned(input: String, options: ParseOptions) -> ParsedDocument {
    ParsedDocument::new(input, options)
}

pub fn parse_cst(input: &str) -> cst::CstDocument {
    cst::parse(input)
}

pub fn parse_cst_owned(input: String) -> cst::CstDocument {
    cst::parse_owned(input)
}

pub fn parse_ast(input: &str, options: ParseOptions) -> AstDocument {
    lower_cst(&parse_cst(input), options)
}

pub fn parse_ast_owned(input: String, options: ParseOptions) -> AstDocument {
    lower_cst(&parse_cst_owned(input), options)
}

pub fn lower_cst(document: &cst::CstDocument, options: ParseOptions) -> AstDocument {
    builder::lower(document, options)
}

pub fn tokenize(input: &str) -> Vec<lexer::TokenSpan> {
    lexer::exported_tokens(input)
}
