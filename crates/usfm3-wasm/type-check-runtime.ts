/**
 * Runtime type validation for usfm3-wasm.
 *
 * Two guarantees in one file:
 *
 *   1. Compile-time (tsc --noEmit): explicit type annotations and exhaustive
 *      union handling cause a type error whenever a declared type diverges from
 *      how the code uses it, or whenever a union gains a new member that isn't
 *      handled here.
 *
 *   2. Runtime (npx tsx): actually executes the WASM and asserts that every
 *      property the declared types promise is present with the right shape.
 *      Catches regressions where Rust serialization changes (e.g. spans format)
 *      without a corresponding TS type update.
 *
 * Run after `wasm-pack build --target nodejs --out-dir pkg-node`:
 *   npx tsc --noEmit --strict --skipLibCheck --moduleResolution bundler --module esnext type-check-runtime.ts
 *   npx tsx type-check-runtime.ts
 *
 * Exit code 0 = all checks passed, 1 = at least one failure.
 */

import { parse, parseCst, parseAst, tokenize } from "./pkg-node/usfm3_wasm.js";
import type {
  AstAttribute,
  AstNode,
  Diagnostic,
  DiagnosticCode,
  ExportedCstNode,
  ParsedAstDocument,
  Severity,
  Span,
  SourceMap,
  SourceSpans,
  TokenSpan,
  UsjAttribute,
  UsjBook,
  UsjChar,
  UsjChapter,
  UsjContentNode,
  UsjDocument,
  UsjFigure,
  UsjMilestone,
  UsjNote,
  UsjOptBreak,
  UsjPara,
  UsjPeriph,
  UsjRef,
  UsjSidebar,
  UsjSpans,
  UsjTable,
  UsjTableCell,
  UsjTableRow,
  UsjUnknown,
  UsjVerse,
} from "./pkg-node/usfm3_wasm.js";

// ── Assertion framework ───────────────────────────────────────────────────────

let failures = 0;

function check(label: string, condition: boolean): void {
  if (!condition) {
    console.error(`FAIL  ${label}`);
    failures++;
  }
}

/** Used in `default` branches to enforce exhaustive union handling at compile time. */
function assertNever(label: string, x: never): never {
  throw new Error(`${label}: unhandled variant ${JSON.stringify(x)}`);
}

/** Runtime guard: { start: number, end: number } object, not an array. */
function isSpan(v: unknown): v is Span {
  return (
    typeof v === "object" &&
    v !== null &&
    !Array.isArray(v) &&
    typeof (v as Span).start === "number" &&
    typeof (v as Span).end === "number"
  );
}

// ── Exhaustive closed-union coverage ─────────────────────────────────────────
// Record<T, true> causes a compile error if T gains a member not listed here.
// These declarations don't run anything; their value is purely in tsc.

const _severity: Record<Severity, true> = {
  info: true,
  warning: true,
  error: true,
};

const _diagnosticCodes: Record<DiagnosticCode, true> = {
  UnknownMarker: true,
  DeprecatedMarker: true,
  UnclosedMarker: true,
  StrayCloseMarker: true,
  MisnestedMarker: true,
  MissingNestingPrefix: true,
  ImplicitClose: true,
  UnclosedNote: true,
  UnclosedAtEof: true,
  InvalidChapterSequence: true,
  InvalidVerseSequence: true,
  DuplicateChapter: true,
  DuplicateId: true,
  MissingIdMarker: true,
  InvalidBookCode: true,
  NoteSubmarkerOutsideNote: true,
  TextBeforeId: true,
  HeaderAfterBody: true,
  MilestoneMismatch: true,
  InvalidAttributes: true,
  MissingChapterNumber: true,
  MissingVerseNumber: true,
  VerseOutsideParagraph: true,
  MissingChapterMarker: true,
  CharCrossesVerseBoundary: true,
  EmptyFigure: true,
  UnquotedAttributeValue: true,
  MissingRequiredAttribute: true,
  DefaultAttributeNotDefined: true,
  BodyParagraphBeforeChapter: true,
  NonEmptyBlankLine: true,
  LeadingZeros: true,
  EmptyWordMarker: true,
  MissingMilestoneSelfClose: true,
  InvalidTableColumnSequence: true,
};

const _tokenKinds: Record<TokenSpan["kind"], true> = {
  whitespace: true,
  marker: true,
  closing_marker: true,
  milestone_end: true,
  attributes: true,
  text: true,
  newline: true,
};

const _tokenSubKinds: Record<NonNullable<TokenSpan["token_kind"]>, true> = {
  chapter: true,
  verse: true,
  milestone: true,
  nested: true,
  regular: true,
};

// Suppress unused-variable warnings; the declarations above exist for tsc.
void _severity, _diagnosticCodes, _tokenKinds, _tokenSubKinds;

// ── Exhaustive USJ node walker ────────────────────────────────────────────────
// The `default: assertNever(...)` branch causes a compile error if a new
// UsjContentNode variant is added without being handled here.

function checkUsjAttr(attr: UsjAttribute): void {
  check("UsjAttribute.key is string", typeof attr.key === "string");
  check("UsjAttribute.value is string", typeof attr.value === "string");
}

function checkUsjSpans(type: string, spans: UsjSpans): void {
  check(`${type}.spans.node is Span (not [n,n])`, isSpan(spans.node));
  if (spans.code !== undefined)
    check(`${type}.spans.code is Span`, isSpan(spans.code));
  if (spans.number !== undefined)
    check(`${type}.spans.number is Span`, isSpan(spans.number));
  if (spans.close !== undefined)
    check(`${type}.spans.close is Span`, isSpan(spans.close));
}

function checkUsjNode(node: UsjContentNode): void {
  if (typeof node === "string") return;

  if (node.type !== "optbreak" && node.spans) checkUsjSpans(node.type, node.spans);

  switch (node.type) {
    case "book": {
      const n: UsjBook = node;
      check("book.marker is string", typeof n.marker === "string");
      check("book.code is string", typeof n.code === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      break;
    }
    case "chapter": {
      const n: UsjChapter = node;
      check("chapter.marker is string", typeof n.marker === "string");
      check("chapter.number is string", typeof n.number === "string");
      if (n.sid !== undefined) check("chapter.sid is string", typeof n.sid === "string");
      if (n.altnumber !== undefined) check("chapter.altnumber is string", typeof n.altnumber === "string");
      if (n.pubnumber !== undefined) check("chapter.pubnumber is string", typeof n.pubnumber === "string");
      break;
    }
    case "verse": {
      const n: UsjVerse = node;
      check("verse.marker is string", typeof n.marker === "string");
      check("verse.number is string", typeof n.number === "string");
      if (n.sid !== undefined) check("verse.sid is string", typeof n.sid === "string");
      if (n.altnumber !== undefined) check("verse.altnumber is string", typeof n.altnumber === "string");
      if (n.pubnumber !== undefined) check("verse.pubnumber is string", typeof n.pubnumber === "string");
      break;
    }
    case "para": {
      const n: UsjPara = node;
      check("para.marker is string", typeof n.marker === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      break;
    }
    case "char": {
      const n: UsjChar = node;
      check("char.marker is string", typeof n.marker === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      for (const a of n.attributes ?? []) checkUsjAttr(a);
      break;
    }
    case "note": {
      const n: UsjNote = node;
      check("note.marker is string", typeof n.marker === "string");
      check("note.caller is string", typeof n.caller === "string");
      if (n.category !== undefined) check("note.category is string", typeof n.category === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      break;
    }
    case "ms": {
      const n: UsjMilestone = node;
      check("ms.marker is string", typeof n.marker === "string");
      for (const a of n.attributes ?? []) checkUsjAttr(a);
      break;
    }
    case "figure": {
      const n: UsjFigure = node;
      check("figure.marker is string", typeof n.marker === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      for (const a of n.attributes ?? []) checkUsjAttr(a);
      break;
    }
    case "sidebar": {
      const n: UsjSidebar = node;
      check("sidebar.marker is string", typeof n.marker === "string");
      if (n.category !== undefined) check("sidebar.category is string", typeof n.category === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      break;
    }
    case "periph": {
      const n: UsjPeriph = node;
      if (n.alt !== undefined) check("periph.alt is string", typeof n.alt === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      for (const a of n.attributes ?? []) checkUsjAttr(a);
      break;
    }
    case "table": {
      const n: UsjTable = node;
      for (const c of n.content ?? []) checkUsjNode(c);
      break;
    }
    case "table:row": {
      const n: UsjTableRow = node;
      check("table:row.marker is string", typeof n.marker === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      break;
    }
    case "table:cell": {
      const n: UsjTableCell = node;
      check("table:cell.marker is string", typeof n.marker === "string");
      check("table:cell.align is string", typeof n.align === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      break;
    }
    case "ref": {
      const n: UsjRef = node;
      for (const c of n.content ?? []) checkUsjNode(c);
      for (const a of n.attributes ?? []) checkUsjAttr(a);
      break;
    }
    case "unknown": {
      const n: UsjUnknown = node;
      check("unknown.marker is string", typeof n.marker === "string");
      for (const c of n.content ?? []) checkUsjNode(c);
      break;
    }
    case "optbreak": {
      const _: UsjOptBreak = node;
      break;
    }
    default:
      assertNever("UsjContentNode.type", node);
  }
}

// ── Exhaustive AST node walker ────────────────────────────────────────────────
// Uses key-based narrowing. TypeScript tracks which variants remain after each
// `in` check, so the final `assertNever` is unreachable only when all variants
// are handled — a compile error otherwise.

function checkAstAttr(attr: AstAttribute): void {
  check("AstAttribute.key is string", typeof attr.key === "string");
  check("AstAttribute.value is string", typeof attr.value === "string");
}

function checkAstNode(node: AstNode): void {
  if (node === "OptBreak") return;
  if ("Book" in node) {
    check("Book.marker is string", typeof node.Book.marker === "string");
    check("Book.code is string", typeof node.Book.code === "string");
    for (const c of node.Book.content) checkAstNode(c);
  } else if ("Chapter" in node) {
    check("Chapter.number is string", typeof node.Chapter.number === "string");
  } else if ("Verse" in node) {
    check("Verse.number is string", typeof node.Verse.number === "string");
  } else if ("Para" in node) {
    for (const c of node.Para.content) checkAstNode(c);
  } else if ("Char" in node) {
    for (const c of node.Char.content) checkAstNode(c);
    for (const a of node.Char.attributes) checkAstAttr(a);
  } else if ("Note" in node) {
    check("Note.caller is string", typeof node.Note.caller === "string");
    for (const c of node.Note.content) checkAstNode(c);
  } else if ("Milestone" in node) {
    for (const a of node.Milestone.attributes) checkAstAttr(a);
  } else if ("Figure" in node) {
    for (const c of node.Figure.content) checkAstNode(c);
    for (const a of node.Figure.attributes) checkAstAttr(a);
  } else if ("Sidebar" in node) {
    for (const c of node.Sidebar.content) checkAstNode(c);
  } else if ("Periph" in node) {
    for (const c of node.Periph.content) checkAstNode(c);
  } else if ("Table" in node) {
    for (const c of node.Table.content) checkAstNode(c);
  } else if ("TableRow" in node) {
    for (const c of node.TableRow.content) checkAstNode(c);
  } else if ("TableCell" in node) {
    for (const c of node.TableCell.content) checkAstNode(c);
  } else if ("Ref" in node) {
    for (const c of node.Ref.content) checkAstNode(c);
  } else if ("Unknown" in node) {
    for (const c of node.Unknown.content) checkAstNode(c);
  } else if ("Text" in node) {
    check("Text is string", typeof node.Text === "string");
  } else {
    assertNever("AstNode", node);
  }
}

// ── Sample USFM ───────────────────────────────────────────────────────────────
// Exercises: book, chapter, verse, para, char (with attributes), note,
// table (row + cell), sidebar, and milestone.

const USFM = `\\id GEN
\\h Genesis
\\c 1
\\p
\\v 1 In the beginning \\w God|lemma="God"\\w* created.\\f + \\ft A footnote.\\f*
\\v 2 The earth was \\nd Lord\\nd* formless.
\\tr \\tc1 Cell one \\tc2 Cell two
\\esb
\\ip Sidebar content.
\\esbe
\\ts\\*`;

// ── tokenize() ────────────────────────────────────────────────────────────────

const tokens: TokenSpan[] = tokenize(USFM);

check("tokenize: returns array", Array.isArray(tokens));
check("TokenSpan.kind is string", typeof tokens[0].kind === "string");
check("TokenSpan.text is string", typeof tokens[0].text === "string");
check("TokenSpan.start is number", typeof tokens[0].start === "number");
check("TokenSpan.end is number", typeof tokens[0].end === "number");
const markerTok = tokens.find((t) => t.kind === "marker");
check("marker token has normalized_marker", markerTok !== undefined && typeof markerTok.normalized_marker === "string");

// ── parseCst() ────────────────────────────────────────────────────────────────

const cst: ExportedCstNode = parseCst(USFM);

check("cst.type is string", typeof cst.type === "string");
check("cst.span is Span object (not array)", isSpan(cst.span));
check("cst.children is array", Array.isArray(cst.children));

// ── parseAst() ────────────────────────────────────────────────────────────────

const parsedAst: ParsedAstDocument = parseAst(USFM, { diagnostics: true });

check("parseAst.ast.content is array", Array.isArray(parsedAst.ast.content));
check("parseAst.sourceMap.content is array", Array.isArray(parsedAst.sourceMap.content));
for (const node of parsedAst.ast.content) checkAstNode(node);

// ── parse() + diagnostics() ───────────────────────────────────────────────────

const doc = parse(USFM, { diagnostics: true });
const diags: Diagnostic[] | undefined = doc.diagnostics();

check("diagnostics() returns array when enabled", Array.isArray(diags));
if (Array.isArray(diags) && diags.length > 0) {
  const d = diags[0];
  check("Diagnostic.severity is string", typeof d.severity === "string");
  check("Diagnostic.span is Span object (not array)", isSpan(d.span));
  check("Diagnostic.message is string", typeof d.message === "string");
  check("Diagnostic.code is string", typeof d.code === "string");
}

// ── sourceMap() ───────────────────────────────────────────────────────────────

const sm: SourceMap = doc.sourceMap();

check("SourceMap.content is array", Array.isArray(sm.content));
const smNode = sm.content.find((n) => n.spans);
if (smNode?.spans) {
  const ss: SourceSpans = smNode.spans;
  check("SourceSpans.node is Span object (not array)", isSpan(ss.node));
}

// ── toUsj() ───────────────────────────────────────────────────────────────────

const usj: UsjDocument = doc.toUsj({ spans: true });

check("UsjDocument.type === 'USJ'", usj.type === "USJ");
check("UsjDocument.version is string", typeof usj.version === "string");
check("UsjDocument.content is array", Array.isArray(usj.content));

for (const node of usj.content) checkUsjNode(node);

// ── Result ────────────────────────────────────────────────────────────────────

if (failures > 0) {
  throw new Error(`${failures} check(s) failed.`);
} else {
  console.log("All checks passed.");
}
