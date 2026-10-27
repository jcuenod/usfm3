// Public types for the `usfm3` npm package.
//
// Hand-written and authoritative: every Rust export carries `skip_typescript`, so the
// generated `usfm3_wasm.d.ts` is not what consumers see. This file lives next to the JS
// it describes (rather than in a `typescript_custom_section` inside lib.rs) so it gets
// real editor tooling — the previous arrangement had silently drifted from the Rust.

/** USFM specification version. Selects the marker table and lint rules. */
export type Version = "3.1" | "3.2";

export interface ParseOptions {
  /** Collect diagnostics (lexical/structural + a semantic validation pass).
   *  Off by default so the hot path pays nothing. */
  diagnostics?: boolean;
  /** USFM version. Defaults to "3.1". */
  version?: Version;
}

// ── Primitives ───────────────────────────────────────────────────────────────

export interface Span {
  start: number;
  end: number;
  /** Present on synthetic close tokens (implicit/recovered closes). */
  synthetic?: boolean;
}

// ── Diagnostics ──────────────────────────────────────────────────────────────

export type Severity = "error" | "warning" | "info";

export type DiagnosticCode =
  | "UnknownMarker"
  | "UnclosedMarker"
  | "StrayCloseMarker"
  | "StrayBackslash"
  | "MissingIdMarker"
  | "InvalidBookCode"
  | "DuplicateId"
  | "TextBeforeId"
  | "MissingChapterMarker"
  | "InvalidChapterSequence"
  | "DuplicateChapter"
  | "InvalidVerseSequence"
  | "VerseOutsideParagraph"
  | "NoteSubmarkerOutsideNote"
  | "NonEmptyBlankLine"
  | "LeadingZeros"
  | "MilestoneMismatch"
  | "InvalidAttributes"
  | "MissingChapterNumber"
  | "MissingVerseNumber"
  | "CharCrossesVerseBoundary"
  | "EmptyFigure"
  | "UnquotedAttributeValue"
  | "MissingRequiredAttribute"
  | "BodyParagraphBeforeChapter"
  | "EmptyWordMarker"
  | "MissingMilestoneSelfClose"
  | "InvalidTableColumnSequence"
  | "DeprecatedMarker"
  | "ImplicitClose"
  | "UnclosedNote"
  | "UnclosedAtEof"
  | "DefaultAttributeNotDefined"
  | "MisplacedNumberMarker"
  | "MissingSpaceBeforeNumber"
  | "MisnestedMarker"
  | "MissingNestingPrefix"
  | "HeaderAfterBody";

/** A diagnostic. `start`/`end`/`column` are in the requested {@link OffsetSpace}
 *  (UTF-16 by default, so they index the source string directly); `line` is
 *  space-independent. Both are 1-based. */
export interface Diagnostic {
  severity: Severity;
  code: DiagnosticCode;
  message: string;
  start: number;
  end: number;
  line: number;
  column: number;
}

// ── CST (concrete syntax tree, for editor decoration) ──────────────────────────

export interface CstClose {
  start: number;
  end: number;
  synthetic?: boolean;
}

export interface CstNode {
  type: string;
  marker?: string;
  category?: string;
  role?: string;
  closing?: string;
  number?: number;
  /** UTF-8 **byte** offsets into the source. Unlike {@link Diagnostic} and
   *  {@link TapeCursor}, `cst()` always reports bytes. */
  start: number;
  end: number;
  synthetic?: boolean;
  close?: CstClose;
  children?: CstNode[];
}

// ── USJ ────────────────────────────────────────────────────────────────────────

export interface UsjAttribute {
  key: string;
  value: string;
}

export interface UsjBook { type: "book"; marker: string; code: string; content?: UsjContentNode[]; }
export interface UsjChapter { type: "chapter"; marker: string; number: string; sid?: string; altnumber?: string; pubnumber?: string; }
export interface UsjVerse { type: "verse"; marker: string; number: string; sid?: string; altnumber?: string; pubnumber?: string; }
export interface UsjPara { type: "para"; marker: string; content?: UsjContentNode[]; }
export interface UsjChar { type: "char"; marker: string; content?: UsjContentNode[]; attributes?: UsjAttribute[]; }
export interface UsjNote { type: "note"; marker: string; caller: string; category?: string; content?: UsjContentNode[]; }
export interface UsjMilestone { type: "ms"; marker: string; attributes?: UsjAttribute[]; }
export interface UsjFigure { type: "figure"; marker: string; content?: UsjContentNode[]; attributes?: UsjAttribute[]; }
export interface UsjSidebar { type: "sidebar"; marker: string; category?: string; content?: UsjContentNode[]; }
export interface UsjPeriph { type: "periph"; alt?: string; id?: string; content?: UsjContentNode[]; }
export interface UsjTable { type: "table"; content?: UsjContentNode[]; }
export interface UsjTableRow { type: "table:row"; marker: string; content?: UsjContentNode[]; }
export interface UsjTableCell { type: "table:cell"; marker: string; align: string; content?: UsjContentNode[]; }
export interface UsjRef { type: "ref"; loc?: string; content?: UsjContentNode[]; }
export interface UsjUnknown { type: "unknown"; marker: string; content?: UsjContentNode[]; }
export interface UsjOptBreak { type: "optbreak"; }

export type UsjContentNode =
  | string
  | UsjBook
  | UsjChapter
  | UsjVerse
  | UsjPara
  | UsjChar
  | UsjNote
  | UsjMilestone
  | UsjFigure
  | UsjSidebar
  | UsjPeriph
  | UsjTable
  | UsjTableRow
  | UsjTableCell
  | UsjRef
  | UsjUnknown
  | UsjOptBreak;

export interface UsjDocument {
  type: "USJ";
  version: string;
  content: UsjContentNode[];
}

// ── Offset spaces ─────────────────────────────────────────────────────────────

/** Coordinate space for source offsets.
 *
 *  `"utf16"` (the default for this package) indexes JS strings directly, so
 *  `source.slice(start, end)` is correct. `"utf8"` gives raw byte offsets, matching
 *  `cst()` and the Rust/CLI API — useful for interop, but slicing a JS string with them
 *  is wrong for any non-ASCII text. `"chars"` counts Unicode scalars. */
export type OffsetSpace = "utf8" | "utf16" | "chars";

export interface OffsetOptions {
  /** Defaults to `"utf16"`. */
  offsets?: OffsetSpace;
}

export interface TapeOptions extends OffsetOptions {
  /** Marker table to interpret the tape against. Defaults to `"3.1"`. */
  version?: Version;
}

// ── Packed tape ───────────────────────────────────────────────────────────────

/** One marker's metadata. `name` is the table **base** (`"q"`, not `"q1"`); the name as
 *  written is {@link TapeCursor.markerName}. */
export interface MarkerMeta {
  name: string;
  category: string;
  role: string;
  closing: string;
  usx: string;
  deprecated: string | null;
}

/** Everything needed to interpret a packed tape. Fetch once per version and cache it;
 *  {@link tapeSchema} does that for you. */
export interface TapeSchema {
  format: number;
  magic: number;
  headerWords: number;
  nodeWords: number;
  version: Version;
  /** Index **is** the packed `kind` code. */
  readonly kinds: readonly TapeKind[];
  readonly flags: Readonly<Record<string, number>>;
  /** `MarkerId::UNKNOWN` — `markers[unknownMarker]` is `undefined`. */
  unknownMarker: number;
  /** Index **is** the packed `marker` id, for a tape built with this `version`. */
  readonly markers: readonly MarkerMeta[];
}

export type TapeKind =
  | "root"
  | "marker"
  | "close"
  | "chapter"
  | "verse"
  | "text"
  | "attribute"
  | "block_end"
  | "error";

/** A packed tape plus the source it refers to. Holds no WebAssembly resources, so it
 *  outlives the {@link ParsedDocument} it came from and can be `postMessage`d (transfer
 *  `words.buffer` alongside the source string). */
export declare class Tape {
  constructor(words: Uint32Array, source: string, schema: TapeSchema);
  readonly words: Uint32Array;
  readonly source: string;
  readonly schema: TapeSchema;
  /** Node count. Larger than the node count of {@link ParsedDocument.cst}, which folds
   *  every close token into its parent's `close` field instead of emitting a node. */
  readonly length: number;
  /** Which space `start`/`end` are in. */
  readonly offsets: OffsetSpace;
  /** Whether `source.slice(start, end)` can be trusted — i.e. whether this tape's
   *  offsets *are* UTF-16 code-unit offsets. True for `"utf16"`, and for the other
   *  spaces only when the source happens to coincide (pure ASCII, or no astral
   *  scalars). When false, {@link TapeCursor.text}, {@link TapeCursor.markerName} and
   *  {@link TapeCursor.toJSON} throw instead of returning wrong characters. */
  readonly sliceable: boolean;
  root(): TapeCursor;
  at(index: number): TapeCursor;
  /** Every node in flat tape order, as independent cursors — safe to spread, filter or
   *  retain. */
  nodes(): IterableIterator<TapeCursor>;
  /** Every node in flat tape order, allocation-free: yields **one reused cursor**, so
   *  never retain or collect it. The fastest full traversal. */
  scan(): IterableIterator<TapeCursor>;
}

/** A flyweight pointer into a {@link Tape}. Every property is read from the buffer on
 *  access, so cursors are cheap to make and free to hold. */
export declare class TapeCursor {
  constructor(tape: Tape, index: number);
  tape: Tape;
  index: number;

  readonly kind: TapeKind;
  readonly kindCode: number;
  readonly flags: number;
  readonly markerId: number;

  readonly start: number;
  readonly end: number;
  readonly length: number;
  /** Raw `link` word — overloaded by kind. Prefer `matchingClose()` / `number`. */
  readonly link: number;

  readonly synthetic: boolean;
  readonly selfClosing: boolean;
  readonly hasAttrs: boolean;
  readonly hasError: boolean;
  readonly nested: boolean;

  /** `source.slice(start, end)`. **Throws** when {@link Tape.sliceable} is false. */
  readonly text: string;
  /** `undefined` for non-marker nodes and for markers absent from this version's table. */
  readonly marker: MarkerMeta | undefined;
  /** The name as written (`"q1"`, `"tcr1-2"`), not the table base. Recovered by
   *  slicing the source, so it **throws** when {@link Tape.sliceable} is false —
   *  structural metadata must not degrade to a plausible-looking `""`. */
  readonly markerName: string;
  /** Chapter/verse number, else `undefined`. */
  readonly number: number | undefined;

  matchingClose(): TapeCursor | null;
  firstChild(): TapeCursor | null;
  nextSibling(): TapeCursor | null;
  children(): IterableIterator<TapeCursor>;
  /** This node and its subtree, preorder. */
  walk(): IterableIterator<TapeCursor>;
  /** The same shape {@link ParsedDocument.cst} produces for this node. Emits `marker`,
   *  so it **throws** when {@link Tape.sliceable} is false. */
  toJSON(): CstNode;
}

export declare function tape(words: Uint32Array, source: string, schema: TapeSchema): Tape;

// ── ParsedDocument handle ──────────────────────────────────────────────────────

export declare class ParsedDocument {
  /** Free the WebAssembly-side handle. Call when done. A {@link Tape} taken from this
   *  document stays valid afterwards; a `cst()` object graph does too. */
  free(): void;
  /** Concrete syntax tree as an object graph, with **byte** offsets. Unchanged. For a
   *  book-sized document on a hot path, prefer {@link parseTape}. */
  cst(): CstNode;
  /** Diagnostics as a flat array. Empty unless `diagnostics: true` was passed. */
  diagnostics(options?: OffsetOptions): Diagnostic[];
  /** The tape as a packed `Uint32Array`: an 8-word header, then 4 words per node.
   *  A copy owned by the JS heap — safe across `free()` and across wasm memory growth,
   *  and transferable to a worker. Pair with {@link tapeSchema}, or use
   *  {@link tapeView} / {@link parseTape}. */
  tape(options?: OffsetOptions): Uint32Array;
  /** USJ document object. */
  toUsj(): UsjDocument;
  /** USX serialization (XML string). */
  toUsx(): string;
  /** Normalized USFM (string). */
  toUsfm(): string;
  /** Verse-reference → cleaned-text dictionary, in document order. */
  toVref(): Record<string, string>;
}

/** Parse USFM into a lazy handle. Never throws on malformed input — the parser
 *  recovers and (when `diagnostics` is on) reports. */
export declare function parse(usfm: string, options?: ParseOptions): ParsedDocument;

/** One-shot convenience: parse and return the concrete syntax tree as objects. */
export declare function parseCst(usfm: string, options?: ParseOptions): CstNode;

/** The marker/kind schema for a version, memoized. The result is frozen and shared. */
export declare function tapeSchema(version?: Version): TapeSchema;

/** A {@link Tape} view over an already-parsed handle.
 *
 *  `source` must be the exact string passed to {@link parse} — the tape holds offsets
 *  into it. Mismatches throw rather than yielding silently wrong spans. */
export declare function tapeView(
  doc: ParsedDocument,
  source: string,
  options?: TapeOptions,
): Tape;

/** Parse and return a walkable {@link Tape}, freeing the handle before returning.
 *
 *  The fast path: no JSON, no object graph, and an allocation-free walk. */
export declare function parseTape(
  usfm: string,
  options?: ParseOptions & TapeOptions,
): Tape;

/** Async initializer (browser/bundler entry). Node's entry calls `initSync` for you. */
export default function init(
  module_or_path?: RequestInfo | URL | Response | BufferSource | WebAssembly.Module,
): Promise<unknown>;
export declare function initSync(module: { module: BufferSource | WebAssembly.Module }): unknown;
