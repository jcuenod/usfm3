/// Classification of USFM markers by structural role.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkerKind {
    /// \p, \q1, \m, \li1, \b, \nb, \pi1, etc.
    /// Implicitly closed by the next paragraph marker.
    Paragraph,

    /// \f, \x -- footnote / cross-reference.
    /// Explicitly closed by \f*, \x*. Contains sub-markers.
    Note,

    /// \nd, \bk, \add, \it, \bd, \sc, etc.
    /// Explicitly closed by \nd*, \bk*, etc.
    Character,

    /// \id, \usfm, \ide, \h, \toc1, \toc2, \toc3, \mt1, etc.
    /// Header/identification markers.
    Header,

    /// \c -- chapter milestone.
    Chapter,

    /// \v -- verse milestone.
    Verse,

    /// Milestone start markers: \qt1-s, \ts-s, etc.
    MilestoneStart,

    /// Milestone end markers: \qt1-e, \ts-e, etc.
    MilestoneEnd,

    /// \esb -- sidebar opening.
    SidebarStart,

    /// \esbe -- sidebar closing.
    SidebarEnd,

    /// \fig -- figure.
    Figure,

    /// \rem, \sts, \restore -- metadata markers.
    Meta,

    /// Anything in the \z namespace or unrecognized.
    Unknown,
}

/// Metadata about a single USFM marker.
pub struct MarkerInfo {
    /// The structural classification of the marker.
    pub kind: MarkerKind,
    /// Is this marker valid as a sub-marker inside a \f or \x note?
    pub valid_in_note: bool,
}

impl MarkerInfo {
    const fn new(kind: MarkerKind) -> Self {
        Self {
            kind,
            valid_in_note: false,
        }
    }

    const fn note_sub(kind: MarkerKind) -> Self {
        Self {
            kind,
            valid_in_note: true,
        }
    }
}

/// Look up the metadata for a USFM marker by name.
///
/// The `name` parameter is the marker WITHOUT the leading backslash
/// (e.g., `"p"`, `"nd"`, `"mt1"`, `"fr"`).
///
/// Returns a [`MarkerInfo`] describing the marker's kind and whether it is
/// valid inside a note span.
pub fn lookup_marker(name: &str) -> MarkerInfo {
    // ----------------------------------------------------------------
    // User-extension namespace: anything starting with 'z' is Unknown.
    // ----------------------------------------------------------------
    if name.starts_with('z') {
        return MarkerInfo::new(MarkerKind::Unknown);
    }

    // ----------------------------------------------------------------
    // Milestone markers: patterns ending in -s (start) or -e (end).
    // ----------------------------------------------------------------
    if name.ends_with("-s") {
        return MarkerInfo::new(MarkerKind::MilestoneStart);
    }
    if name.ends_with("-e") {
        return MarkerInfo::new(MarkerKind::MilestoneEnd);
    }

    match name {
        // =============================================================
        // Header / Identification markers
        // =============================================================
        "id" | "usfm" | "ide"
        | "h" | "h1" | "h2" | "h3"
        | "toc1" | "toc2" | "toc3"
        | "toca1" | "toca2" | "toca3"
        | "mt" | "mt1" | "mt2" | "mt3" | "mt4"
        | "mte" | "mte1" | "mte2"
        | "imt" | "imt1" | "imt2" | "imt3" | "imt4"
        | "is" | "is1" | "is2" | "is3"
        | "cl" | "cp" | "cd"
        => MarkerInfo::new(MarkerKind::Header),

        // =============================================================
        // Paragraph markers
        // =============================================================

        // -- body paragraphs --
        "p" | "m" | "po" | "pr" | "cls"
        | "pmo" | "pm" | "pmc" | "pmr"
        | "pi" | "pi1" | "pi2" | "pi3"
        | "mi" | "nb" | "pc"
        | "ph" | "ph1" | "ph2" | "ph3"
        | "b"

        // -- poetry --
        | "q" | "q1" | "q2" | "q3" | "q4"
        | "qr" | "qc" | "qa"
        | "qm" | "qm1" | "qm2" | "qm3"
        | "qd"

        // -- lists --
        | "lh"
        | "li" | "li1" | "li2" | "li3" | "li4"
        | "lf"
        | "lim" | "lim1" | "lim2" | "lim3"

        // -- sections / headings --
        | "ms" | "ms1" | "ms2" | "ms3"
        | "mr"
        | "s" | "s1" | "s2" | "s3" | "s4"
        | "sr" | "r" | "sp"
        | "sd" | "sd1" | "sd2" | "sd3" | "sd4"

        // -- descriptive title (Psalms) --
        | "d"

        // -- introduction paragraphs --
        | "ip" | "ipi" | "im" | "imi"
        | "ipq" | "imq" | "ipr"

        // -- introduction outline --
        | "iot"
        | "io" | "io1" | "io2" | "io3" | "io4"

        // -- introduction lists --
        | "ili" | "ili1" | "ili2"

        // -- introduction end --
        | "ie"

        // -- table row --
        | "tr"

        // -- peripheral content --
        | "periph"
        => MarkerInfo::new(MarkerKind::Paragraph),

        // =============================================================
        // Note markers (footnote, endnote, cross-reference)
        // =============================================================
        "f" | "fe" | "x"
        => MarkerInfo::new(MarkerKind::Note),

        // =============================================================
        // Note sub-markers (Character kind, valid_in_note = true)
        // =============================================================
        "fr" | "ft" | "fk" | "fq" | "fqa" | "fl" | "fw" | "fp" | "fv" | "fdc"
        | "xop" | "xot" | "xnt" | "xdc"
        => MarkerInfo::note_sub(MarkerKind::Character),

        // These markers are character markers that are ALSO valid inside
        // notes (\x cross-references).
        "xo" | "xt" | "xta" | "xk" | "xq"
        => MarkerInfo::note_sub(MarkerKind::Character),

        // =============================================================
        // Character markers (not valid in notes by default)
        // =============================================================

        // -- special text --
        "add" | "bk" | "dc" | "ior" | "iqt"
        | "k" | "litl" | "nd" | "ord"
        | "pn" | "png" | "qt" | "sig"
        | "sls" | "tl" | "wj"

        // -- formatting --
        | "em" | "bd" | "bdit" | "it" | "no" | "sc" | "sup" | "rb"

        // -- word-level / glossary --
        | "pro" | "w" | "wg" | "wh" | "wa"

        // -- references / annotations --
        | "rq" | "ca" | "va" | "vp"

        // -- linking --
        | "jmp"

        // -- table cells (character markers within \tr) --
        | "th1" | "th2" | "th3"
        | "tc1" | "tc2" | "tc3"
        | "thr1" | "thr2" | "thr3"
        | "tcr1" | "tcr2" | "tcr3"
        => MarkerInfo::new(MarkerKind::Character),

        // =============================================================
        // Chapter
        // =============================================================
        "c" => MarkerInfo::new(MarkerKind::Chapter),

        // =============================================================
        // Verse
        // =============================================================
        "v" => MarkerInfo::new(MarkerKind::Verse),

        // =============================================================
        // Figure
        // =============================================================
        "fig" => MarkerInfo::new(MarkerKind::Figure),

        // =============================================================
        // Sidebar
        // =============================================================
        "esb"  => MarkerInfo::new(MarkerKind::SidebarStart),
        "esbe" => MarkerInfo::new(MarkerKind::SidebarEnd),

        // =============================================================
        // Meta markers
        // =============================================================
        "rem" | "sts" | "restore" | "lit" | "cat"
        => MarkerInfo::new(MarkerKind::Meta),

        // =============================================================
        // Unknown / unrecognized
        // =============================================================
        _ => MarkerInfo::new(MarkerKind::Unknown),
    }
}

// =====================================================================
// Unit tests
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------
    // Header markers
    // -----------------------------------------------------------------
    #[test]
    fn header_markers() {
        let headers = [
            "id", "usfm", "ide", "h", "h1", "h2", "h3",
            "toc1", "toc2", "toc3", "toca1", "toca2", "toca3",
            "mt", "mt1", "mt2", "mt3", "mt4",
            "mte", "mte1", "mte2",
            "imt", "imt1", "imt2", "imt3", "imt4",
            "is", "is1", "is2", "is3",
            "cl", "cp", "cd",
        ];
        for marker in &headers {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::Header,
                "expected Header for \\{}",
                marker,
            );
            assert!(
                !info.valid_in_note,
                "\\{} should not be valid_in_note",
                marker,
            );
        }
    }

    // -----------------------------------------------------------------
    // Paragraph markers
    // -----------------------------------------------------------------
    #[test]
    fn paragraph_markers() {
        let paragraphs = [
            "p", "m", "po", "pr", "cls", "pmo", "pm", "pmc", "pmr",
            "pi", "pi1", "pi2", "pi3", "mi", "nb", "pc",
            "ph", "ph1", "ph2", "ph3", "b",
            "q", "q1", "q2", "q3", "q4", "qr", "qc", "qa",
            "qm", "qm1", "qm2", "qm3", "qd",
            "lh", "li", "li1", "li2", "li3", "li4", "lf",
            "lim", "lim1", "lim2", "lim3",
            "ms", "ms1", "ms2", "ms3", "mr",
            "s", "s1", "s2", "s3", "s4", "sr", "r", "sp",
            "sd", "sd1", "sd2", "sd3", "sd4",
            "d",
            "ip", "ipi", "im", "imi", "ipq", "imq", "ipr",
            "iot", "io", "io1", "io2", "io3", "io4",
            "ili", "ili1", "ili2",
            "ie",
            "tr",
            "periph",
        ];
        for marker in &paragraphs {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::Paragraph,
                "expected Paragraph for \\{}",
                marker,
            );
        }
    }

    // -----------------------------------------------------------------
    // Note markers
    // -----------------------------------------------------------------
    #[test]
    fn note_markers() {
        for marker in &["f", "fe", "x"] {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::Note,
                "expected Note for \\{}",
                marker,
            );
            assert!(
                !info.valid_in_note,
                "\\{} itself should not be valid_in_note",
                marker,
            );
        }
    }

    // -----------------------------------------------------------------
    // Note sub-markers (valid_in_note = true)
    // -----------------------------------------------------------------
    #[test]
    fn note_sub_markers_valid_in_note() {
        let note_subs = [
            "fr", "ft", "fk", "fq", "fqa", "fl", "fw", "fp", "fv", "fdc",
            "xo", "xop", "xt", "xta", "xk", "xq", "xot", "xnt", "xdc",
        ];
        for marker in &note_subs {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::Character,
                "expected Character for note sub-marker \\{}",
                marker,
            );
            assert!(
                info.valid_in_note,
                "\\{} should be valid_in_note",
                marker,
            );
        }
    }

    // -----------------------------------------------------------------
    // Character markers (not valid in notes)
    // -----------------------------------------------------------------
    #[test]
    fn character_markers() {
        let chars = [
            "add", "bk", "dc", "ior", "iqt", "k", "litl", "nd", "ord",
            "pn", "png", "qt", "sig", "sls", "tl", "wj",
            "em", "bd", "bdit", "it", "no", "sc", "sup", "rb",
            "pro", "w", "wg", "wh", "wa",
            "rq", "ca", "va", "vp",
            "jmp",
            "th1", "th2", "th3", "tc1", "tc2", "tc3",
            "thr1", "thr2", "thr3", "tcr1", "tcr2", "tcr3",
        ];
        for marker in &chars {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::Character,
                "expected Character for \\{}",
                marker,
            );
            assert!(
                !info.valid_in_note,
                "\\{} should not be valid_in_note",
                marker,
            );
        }
    }

    // -----------------------------------------------------------------
    // Chapter and Verse
    // -----------------------------------------------------------------
    #[test]
    fn chapter_marker() {
        let info = lookup_marker("c");
        assert_eq!(info.kind, MarkerKind::Chapter);
        assert!(!info.valid_in_note);
    }

    #[test]
    fn verse_marker() {
        let info = lookup_marker("v");
        assert_eq!(info.kind, MarkerKind::Verse);
        assert!(!info.valid_in_note);
    }

    // -----------------------------------------------------------------
    // Figure
    // -----------------------------------------------------------------
    #[test]
    fn figure_marker() {
        let info = lookup_marker("fig");
        assert_eq!(info.kind, MarkerKind::Figure);
        assert!(!info.valid_in_note);
    }

    // -----------------------------------------------------------------
    // Sidebar
    // -----------------------------------------------------------------
    #[test]
    fn sidebar_markers() {
        let info = lookup_marker("esb");
        assert_eq!(info.kind, MarkerKind::SidebarStart);

        let info = lookup_marker("esbe");
        assert_eq!(info.kind, MarkerKind::SidebarEnd);
    }

    // -----------------------------------------------------------------
    // Meta markers
    // -----------------------------------------------------------------
    #[test]
    fn meta_markers() {
        for marker in &["rem", "sts", "restore", "lit", "cat"] {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::Meta,
                "expected Meta for \\{}",
                marker,
            );
        }
    }

    // -----------------------------------------------------------------
    // Milestone markers
    // -----------------------------------------------------------------
    #[test]
    fn milestone_start_markers() {
        for marker in &["qt1-s", "qt2-s", "ts-s", "foo-s"] {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::MilestoneStart,
                "expected MilestoneStart for \\{}",
                marker,
            );
        }
    }

    #[test]
    fn milestone_end_markers() {
        for marker in &["qt1-e", "qt2-e", "ts-e", "foo-e"] {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::MilestoneEnd,
                "expected MilestoneEnd for \\{}",
                marker,
            );
        }
    }

    // -----------------------------------------------------------------
    // z-prefixed user extensions -> Unknown
    // -----------------------------------------------------------------
    #[test]
    fn z_namespace_is_unknown() {
        for marker in &["zanything", "zcustom", "z", "zmymarker"] {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::Unknown,
                "expected Unknown for \\{}",
                marker,
            );
        }
    }

    // -----------------------------------------------------------------
    // Unrecognized markers -> Unknown
    // -----------------------------------------------------------------
    #[test]
    fn unknown_markers() {
        for marker in &["notamarker", "xyz", "hello", "foobar"] {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::Unknown,
                "expected Unknown for \\{}",
                marker,
            );
        }
    }
}
