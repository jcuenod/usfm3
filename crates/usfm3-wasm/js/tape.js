/**
 * Decoder and cursor for the packed tape.
 *
 * This module imports nothing — not even the wasm glue. That is the point: a `Tape`
 * needs only the buffer, the source string, and the schema, so it works on a thread
 * that received those three by `postMessage` with no wasm instance at all, and it can
 * be unit-tested without initializing wasm.
 *
 * Everything about the wire format comes from the schema (`magic`, `headerWords`,
 * `nodeWords`, `kinds`, `flags`), so no constant here duplicates one in Rust.
 *
 * The traversal below deliberately mirrors `crates/usfm3/src/cst.rs`. That duplication
 * is the real cost of handing over a flat tape, and it is contained by a test that
 * compares `root().toJSON()` against `cst()` over the whole conformance corpus — if
 * these rules drift from `Cursor`, that test fails.
 */

/** Kinds whose `marker` field indexes the schema's marker table. */
const MARKER_BEARING = new Set(["marker", "close", "block_end", "chapter", "verse"]);
/** Kinds that terminate a run of children (a parent's close token). */
const CLOSING = new Set(["close", "block_end"]);
/** Kinds whose `link` is a jump pointer to a close rather than a number or self-index. */
const CONTAINER = new Set(["root", "marker"]);
/** Kinds whose `link` holds a chapter/verse number. */
const NUMBERED = new Set(["chapter", "verse"]);

/**
 * A packed tape plus the source it refers to.
 *
 * @param {Uint32Array} words  from `ParsedDocument.tape()`
 * @param {string} source      the exact string that was parsed
 * @param {object} schema      from `tapeSchema(version)`
 */
export class Tape {
  constructor(words, source, schema) {
    if (!(words instanceof Uint32Array)) {
      throw new TypeError("tape: expected a Uint32Array from ParsedDocument.tape()");
    }
    if (typeof source !== "string") {
      throw new TypeError("tape: expected the source string that was parsed");
    }
    if (!schema || !Array.isArray(schema.kinds) || !Array.isArray(schema.markers)) {
      throw new TypeError("tape: expected a schema from tapeSchema()");
    }

    const { headerWords, nodeWords, magic, format } = schema;
    if (words.length < headerWords) throw new RangeError("tape: buffer is shorter than its header");
    if (words[0] !== magic) throw new RangeError("tape: bad magic — not a packed tape");
    if (words[1] !== format) {
      throw new RangeError(`tape: wire format ${words[1]} but schema describes ${format}`);
    }

    const count = words[2];
    if (words.length !== headerWords + count * nodeWords) {
      throw new RangeError(
        `tape: buffer holds ${words.length} words, expected ${headerWords + count * nodeWords} for ${count} nodes`,
      );
    }
    if (words[4] !== versionRank(schema.version)) {
      throw new RangeError(
        `tape: built for USFM version rank ${words[4]}, schema is for ${schema.version}`,
      );
    }

    // The header carries the source length in all three spaces, so this check is O(1)
    // whichever space the offsets are in — and it is worth doing, because every span
    // in the buffer is meaningless against the wrong string.
    this.offsets = ["utf8", "utf16", "chars"][words[3]];
    const expected = { utf8: words[5], utf16: words[6], chars: words[7] }[this.offsets];
    const actual = this.offsets === "utf16" ? source.length : measure(source, this.offsets);
    if (actual !== expected) {
      throw new RangeError(
        `tape: source is ${actual} ${this.offsets} units but the tape was built from ${expected}` +
          " — wrong string, or it was mutated after parsing",
      );
    }

    // Can `source.slice(start, end)` be trusted? Only if this tape's offsets *are*
    // UTF-16 code-unit offsets, which is another O(1) comparison of header lengths:
    // `bytes === utf16` means the source is pure ASCII, and `chars === utf16` means it
    // has no astral scalars. Anything else and slicing would silently return the wrong
    // characters, so `text` / `markerName` / `toJSON` refuse rather than lie.
    this.sliceable = this.offsets === "utf16" || expected === words[6];

    this.words = words;
    this.source = source;
    this.schema = schema;
    this.length = count;
    this._base = headerWords;
    this._stride = nodeWords;
    this._synthetic = schema.flags.synthetic;
  }

  /**
   * Guard for the accessors that slice the source. Called by `TapeCursor.text` and
   * `markerName` (and so by `toJSON`).
   */
  _requireSliceable(what) {
    if (this.sliceable) return;
    throw new RangeError(
      `tape: ${what} needs offsets in the host's own space, but this tape uses ` +
        `"${this.offsets}" offsets over a source that is not plain ASCII — ` +
        "source.slice() would return the wrong characters. Rebuild the view without " +
        '{ offsets: "' +
        this.offsets +
        '" } (utf16 is the default), or read start/end/kind/marker only.',
    );
  }

  /** Cursor at the synthetic document root (node 0). */
  root() {
    return new TapeCursor(this, 0);
  }

  /** Cursor at an arbitrary node index. */
  at(index) {
    if (!Number.isInteger(index) || index < 0 || index >= this.length) {
      throw new RangeError(`tape: node index ${index} out of range (0..${this.length})`);
    }
    return new TapeCursor(this, index);
  }

  /**
   * Every node in flat tape order, as independent cursors — so `[...tape.nodes()]`,
   * `.filter()`, `.find()` and friends all behave.
   *
   * A cursor is two fields, so this is cheap; when the allocation genuinely matters
   * (a full walk on every keystroke) use `scan()` instead.
   */
  *nodes() {
    for (let i = 0; i < this.length; i++) yield new TapeCursor(this, i);
  }

  /**
   * Every node in flat tape order, allocation-free: **one cursor is reused**, its
   * `index` advanced in place. The fastest possible full traversal.
   *
   * Because the same object is yielded every time, it must not be retained or
   * collected — `[...tape.scan()]` is an array of N references to one cursor sitting at
   * the last node. Read what you need inside the loop, or use `nodes()`.
   */
  *scan() {
    const cursor = new TapeCursor(this, 0);
    for (let i = 0; i < this.length; i++) {
      cursor.index = i;
      yield cursor;
    }
  }
}

/**
 * A flyweight pointer into a `Tape`. Holds two fields; every property is computed from
 * the buffer on access, so creating one is nearly free and holding one costs nothing.
 */
export class TapeCursor {
  constructor(tape, index) {
    this.tape = tape;
    this.index = index;
  }

  /** Offset of this node's first word. */
  get _w() {
    return this.tape._base + this.index * this.tape._stride;
  }

  /** Packed `kind | flags << 8 | marker << 16`. */
  get kindCode() {
    return this.tape.words[this._w] & 0xff;
  }
  get flags() {
    return (this.tape.words[this._w] >>> 8) & 0xff;
  }
  get markerId() {
    return this.tape.words[this._w] >>> 16;
  }

  /** Kind name, e.g. `"marker"`, `"text"`, `"close"`. */
  get kind() {
    return this.tape.schema.kinds[this.kindCode];
  }

  get start() {
    return this.tape.words[this._w + 1];
  }
  get length() {
    return this.tape.words[this._w + 2];
  }
  get end() {
    return this.start + this.length;
  }
  /** Raw `link` word. Overloaded by kind — prefer `matchingClose()` / `number`. */
  get link() {
    return this.tape.words[this._w + 3];
  }

  get synthetic() {
    return (this.flags & this.tape._synthetic) !== 0;
  }
  get selfClosing() {
    return (this.flags & this.tape.schema.flags.selfClosing) !== 0;
  }
  get hasAttrs() {
    return (this.flags & this.tape.schema.flags.hasAttrs) !== 0;
  }
  get hasError() {
    return (this.flags & this.tape.schema.flags.hasError) !== 0;
  }
  get nested() {
    return (this.flags & this.tape.schema.flags.nested) !== 0;
  }

  /**
   * Source text this node covers.
   *
   * Throws when this tape's offsets are not the host's own — `source.slice()` would
   * quietly return the wrong characters, and a wrong string is worse than an error.
   * See `Tape#sliceable`.
   */
  get text() {
    this.tape._requireSliceable("text");
    return this.tape.source.slice(this.start, this.end);
  }

  /**
   * Marker metadata, or `undefined` for non-marker nodes and unknown markers.
   *
   * `markerId` is `65535` for `MarkerId::UNKNOWN`, and out-of-range indexing yields
   * `undefined` — which is exactly what `MarkerTable::info` returns in Rust.
   */
  get marker() {
    if (!MARKER_BEARING.has(this.kind)) return undefined;
    return this.tape.schema.markers[this.markerId];
  }

  /**
   * The marker's name *as written* (`q1`, `tcr1-2`, `+nd` -> `nd`), which differs from
   * `marker.name` — the schema stores the table's base (`q`, `tcr`). Mirrors
   * `Cursor::marker_name`.
   *
   * Recovered by slicing the source, so it carries the same requirement as `text` and
   * throws under a non-host offset space. Unlike `text` there is no reading of this as
   * an acceptable caveat: a marker name is structural metadata, a consumer mapping onto
   * a byte-oriented protocol legitimately needs byte offsets *and* names, and the
   * failure mode was a plausible-looking `""` or `"1"` rather than an error.
   */
  get markerName() {
    let s = this.text;
    if (s.startsWith("\\")) s = s.slice(1);
    if (s.startsWith("+")) s = s.slice(1);
    const cut = s.search(/[\s*|\\]/);
    return cut === -1 ? s : s.slice(0, cut);
  }

  /** Chapter/verse number, or `undefined`. For these kinds `link` *is* the number. */
  get number() {
    return NUMBERED.has(this.kind) ? this.link : undefined;
  }

  /** The matching close token of a `marker`, so an editor can decorate `\xxx*` too. */
  matchingClose() {
    return this.kind === "marker" ? this.tape.at(this.link) : null;
  }

  /** First child, or `null`. Mirrors `Cursor::first_child`. */
  firstChild() {
    if (!CONTAINER.has(this.kind)) return null;
    const child = this.index + 1;
    // `link` is the matching close; adjacent means no children. Note the root's `link`
    // is the last node index rather than a close node, which this still handles.
    return child < this.link ? this.tape.at(child) : null;
  }

  /** Next sibling, skipping this node's whole subtree. Mirrors `Cursor::next_sibling`. */
  nextSibling() {
    const after = CONTAINER.has(this.kind) ? this.link + 1 : this.index + 1;
    if (after >= this.tape.length) return null;
    const next = this.tape.at(after);
    // The parent's close terminates the run of children.
    return CLOSING.has(next.kind) ? null : next;
  }

  /** Direct children, in order. */
  *children() {
    for (let c = this.firstChild(); c !== null; c = c.nextSibling()) yield c;
  }

  /** This node and its whole subtree, in preorder. */
  *walk() {
    yield this;
    for (const child of this.children()) yield* child.walk();
  }

  /**
   * The same object shape `ParsedDocument.cst()` produces, so a caller can drop the
   * packed path in without changing downstream code — and so the corpus test can
   * compare the two directly.
   *
   * Emits `marker`, so it inherits `markerName`'s requirement and throws when this
   * tape's offsets are not the host's. A `CstNode` missing its marker names would not be
   * the shape it claims to be.
   */
  toJSON() {
    const out = { type: this.kind };

    const marker = this.marker;
    if (marker) {
      out.marker = this.markerName;
      out.category = marker.category;
      out.role = marker.role;
      out.closing = marker.closing;
    }
    if (NUMBERED.has(this.kind)) out.number = this.number;

    out.start = this.start;
    out.end = this.end;
    if (this.synthetic) out.synthetic = true;

    const close = this.matchingClose();
    if (close) {
      out.close = { start: close.start, end: close.end };
      if (close.synthetic) out.close.synthetic = true;
    }

    const children = [...this.children()].map((c) => c.toJSON());
    if (children.length > 0) out.children = children;

    return out;
  }
}

/** Convenience wrapper matching the class constructor. */
export function tape(words, source, schema) {
  return new Tape(words, source, schema);
}

/** `Version` rank as stored in header word 4. Mirrors `Version::rank`. */
function versionRank(version) {
  return version === "3.2" ? 2 : 1;
}

/**
 * Source length in a non-UTF-16 space, for the constructor's validation check.
 * Only reached under `{ offsets: "utf8" }` or `"chars"`; the `utf16` default uses
 * `source.length` and costs nothing.
 */
function measure(source, space) {
  if (space === "chars") {
    let n = 0;
    // Iterating a string yields code points, so surrogate pairs count once.
    for (const _ of source) n++;
    return n;
  }
  // utf8
  let bytes = 0;
  for (const ch of source) {
    const cp = ch.codePointAt(0);
    bytes += cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
  }
  return bytes;
}
