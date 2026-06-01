//! Stage 1 — structural scan.
//!
//! The only bytes that can begin a structural event in USFM are `\` (marker
//! start) and, secondarily, `|` (attribute separator) and `\n` (block-relevant
//! whitespace). We use `memchr` — SIMD-accelerated on stable Rust, running at
//! roughly memory bandwidth — to find them. The marker count also lets stage 2
//! `reserve` the tape in a single allocation.
//!
//! A hand-rolled `std::simd` / AVX2+NEON path can replace this behind a feature
//! flag later; the stage-2 contract (a stream of candidate offsets) stays the same.

/// Upper-bound estimate of tape nodes, used to pre-reserve. Each `\` yields at
/// most a marker open (+ maybe a close/attr); text runs sit between them.
#[inline]
pub fn estimate_nodes(source: &[u8]) -> usize {
    // ~2 nodes per backslash (open + following text) + root + slack.
    let backslashes = memchr::memchr_iter(b'\\', source).count();
    backslashes * 2 + 8
}

/// Iterator over the byte offsets of every `\` in the source.
/// (Reserved for the hand-rolled SIMD stage-1 path; `build` currently scans inline.)
#[allow(dead_code)]
#[inline]
pub fn marker_offsets(source: &[u8]) -> impl Iterator<Item = usize> + '_ {
    memchr::memchr_iter(b'\\', source)
}
