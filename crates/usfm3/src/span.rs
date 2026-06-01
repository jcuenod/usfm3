//! Byte-offset spans into the original source, with lazy line/column resolution.
//!
//! Every node on the [`crate::tape::Tape`] and every [`crate::diagnostic::Diagnostic`]
//! refers to source by `Span`. We deliberately keep spans as raw byte offsets (`u32`)
//! rather than line/column pairs: it is one of the things that keeps the hot path
//! allocation- and branch-free. Line/column is computed only when a human needs it.

/// A half-open byte range `[start, start + len)` into the source buffer.
///
/// `u32` caps a single document at 4 GiB, which is far beyond any real USFM file
/// and keeps [`crate::tape::Node`] at 16 bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    #[inline]
    pub const fn new(start: u32, len: u32) -> Self {
        Self { start, len }
    }

    #[inline]
    pub const fn empty(at: u32) -> Self {
        Self { start: at, len: 0 }
    }

    #[inline]
    pub const fn end(self) -> u32 {
        self.start + self.len
    }

    #[inline]
    pub const fn range(self) -> core::ops::Range<usize> {
        self.start as usize..self.end() as usize
    }

    /// Zero-copy slice of the source this span points at.
    #[inline]
    pub fn text<'s>(self, source: &'s str) -> &'s str {
        &source[self.range()]
    }

    /// Resolve to 1-based line/column. O(span.start); call only for diagnostics/UI.
    pub fn line_col(self, source: &str) -> LineCol {
        let prefix = &source.as_bytes()[..self.start as usize];
        let line = 1 + memchr::memchr_iter(b'\n', prefix).count() as u32;
        let line_start = memchr::memrchr(b'\n', prefix).map_or(0, |i| i + 1);
        let col = 1 + (self.start as usize - line_start) as u32;
        LineCol { line, col }
    }
}

/// 1-based line and column, resolved on demand from a [`Span`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineCol {
    pub line: u32,
    pub col: u32,
}
