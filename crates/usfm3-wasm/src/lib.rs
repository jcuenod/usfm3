use serde::{Deserialize, Serialize};
use tsify_next::Tsify;
use wasm_bindgen::prelude::*;

use ::usfm3 as usfm3_lib;

#[derive(Tsify, Deserialize, Clone, Copy, Default)]
#[tsify(from_wasm_abi)]
pub struct ParseOptions {
    #[serde(default)]
    pub diagnostics: bool,
}

#[derive(Tsify, Deserialize, Clone, Copy, Default)]
#[tsify(from_wasm_abi)]
pub struct UsjOptions {
    #[serde(default)]
    pub spans: bool,
}

#[wasm_bindgen(typescript_custom_section)]
const TYPESCRIPT_API: &str = r#"
export interface ParseOptions {
  diagnostics?: boolean;
}

export interface UsjOptions {
  spans?: boolean;
}

export class ParsedDocument {
  free(): void;
  cst(): any;
  ast(): any;
  sourceMap(): any;
  diagnostics(): any[] | undefined;
  toUsj(options?: UsjOptions): any;
  toUsx(): string;
  toUsfm(): string;
  toVref(): Record<string, string>;
}

export function parse(usfm: string, options?: ParseOptions): ParsedDocument;
export function parseCst(usfm: string): any;
export function parseAst(usfm: string, options?: ParseOptions): any;
export function tokenize(usfm: string): any[];
"#;

#[wasm_bindgen(skip_typescript)]
pub struct ParsedDocument {
    inner: usfm3_lib::ParsedDocument,
}

#[wasm_bindgen]
impl ParsedDocument {
    #[wasm_bindgen]
    pub fn cst(&self) -> Result<JsValue, JsError> {
        to_js_value(&usfm3_lib::cst::export(self.inner.cst()))
    }

    #[wasm_bindgen]
    pub fn ast(&self) -> Result<JsValue, JsError> {
        to_js_value(self.inner.ast())
    }

    #[wasm_bindgen(js_name = "sourceMap")]
    pub fn source_map(&self) -> Result<JsValue, JsError> {
        to_js_value(self.inner.source_map())
    }

    #[wasm_bindgen]
    pub fn diagnostics(&self) -> Result<JsValue, JsError> {
        to_js_value(&self.inner.diagnostics())
    }

    #[wasm_bindgen(js_name = "toUsj")]
    pub fn to_usj(&self, options: Option<UsjOptions>) -> Result<JsValue, JsError> {
        let options = options.unwrap_or_default();
        let value = self
            .inner
            .to_usj(usfm3_lib::usj::UsjOptions {
                include_spans: options.spans,
            })
            .map_err(|error| JsError::new(&error.to_string()))?;
        to_js_value(&value)
    }

    #[wasm_bindgen(js_name = "toUsx")]
    pub fn to_usx(&self) -> Result<String, JsError> {
        self.inner
            .to_usx()
            .map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(js_name = "toUsfm")]
    pub fn to_usfm(&self) -> String {
        self.inner.to_usfm()
    }

    #[wasm_bindgen(js_name = "toVref")]
    pub fn to_vref(&self) -> Result<JsValue, JsError> {
        to_js_value(&self.inner.to_vref())
    }
}

#[wasm_bindgen(skip_typescript)]
pub fn parse(usfm: &str, options: Option<ParseOptions>) -> Result<ParsedDocument, JsError> {
    let options = options.unwrap_or_default();
    Ok(ParsedDocument {
        inner: usfm3_lib::parse(
            usfm,
            usfm3_lib::ParseOptions {
                diagnostics: options.diagnostics,
            },
        ),
    })
}

#[wasm_bindgen(skip_typescript, js_name = "parseCst")]
pub fn parse_cst(usfm: &str) -> Result<JsValue, JsError> {
    let cst = usfm3_lib::parse_cst(usfm);
    to_js_value(&usfm3_lib::cst::export(&cst))
}

#[wasm_bindgen(skip_typescript, js_name = "parseAst")]
pub fn parse_ast(usfm: &str, options: Option<ParseOptions>) -> Result<JsValue, JsError> {
    let options = options.unwrap_or_default();
    let ast_document = usfm3_lib::parse_ast(
        usfm,
        usfm3_lib::ParseOptions {
            diagnostics: options.diagnostics,
        },
    );
    to_js_value(&ast_document)
}

#[wasm_bindgen(skip_typescript)]
pub fn tokenize(usfm: &str) -> Result<JsValue, JsError> {
    to_js_value(&usfm3_lib::tokenize(usfm))
}

fn to_js_value<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(value).map_err(|error| JsError::new(&error.to_string()))
}
