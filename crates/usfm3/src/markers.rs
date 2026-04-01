use serde::Serialize;
use std::ops::Deref;

/// Classification of USFM markers by structural role.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
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

    /// \periph -- peripheral content section container.
    Periph,

    /// \tr -- table row.
    /// Implicitly closed by the next \tr or paragraph marker.
    TableRow,

    /// \th1, \tc1, \thr1, \tcr1 -- table cells within \tr rows.
    /// Implicitly closed by the next table cell marker within the same row.
    TableCell,

    /// Anything in the \z namespace or unrecognized.
    Unknown,
}

/// Metadata about a single USFM marker.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
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

/// An interned, exact known marker definition.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct KnownMarker {
    name: Box<str>,
    info: MarkerInfo,
}

impl KnownMarker {
    fn new(name: impl Into<Box<str>>, info: MarkerInfo) -> Self {
        Self {
            name: name.into(),
            info,
        }
    }

    pub fn as_str(&self) -> &str {
        self.name.as_ref()
    }

    pub fn kind(&self) -> MarkerKind {
        self.info.kind
    }

    pub fn valid_in_note(&self) -> bool {
        self.info.valid_in_note
    }

    pub fn info(&self) -> MarkerInfo {
        self.info
    }
}

/// Marker names stored in hot-path CST/AST nodes.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub enum MarkerName {
    Known(KnownMarker),
    Custom(Box<str>),
}

impl MarkerName {
    pub fn parse(name: &str) -> Self {
        lookup_known_marker_exact(name)
            .map(Self::Known)
            .unwrap_or_else(|| Self::Custom(name.into()))
    }

    pub fn as_str(&self) -> &str {
        match self {
            MarkerName::Known(marker) => marker.as_str(),
            MarkerName::Custom(marker) => marker.as_ref(),
        }
    }

    pub fn kind(&self) -> MarkerKind {
        match self {
            MarkerName::Known(marker) => marker.kind(),
            MarkerName::Custom(marker) => lookup_marker(marker).kind,
        }
    }

    pub fn valid_in_note(&self) -> bool {
        match self {
            MarkerName::Known(marker) => marker.valid_in_note(),
            MarkerName::Custom(marker) => lookup_marker(marker).valid_in_note,
        }
    }

    pub fn default_attribute(&self) -> Option<&'static str> {
        default_attribute(self.as_str())
    }

    pub fn required_attributes(&self) -> &'static [&'static str] {
        required_attributes(self.as_str())
    }
}

impl Deref for MarkerName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl AsRef<str> for MarkerName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl std::fmt::Display for MarkerName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for MarkerName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl From<&str> for MarkerName {
    fn from(value: &str) -> Self {
        Self::parse(value)
    }
}

impl From<String> for MarkerName {
    fn from(value: String) -> Self {
        lookup_known_marker_exact(&value)
            .map(Self::Known)
            .unwrap_or_else(|| Self::Custom(value.into_boxed_str()))
    }
}

impl From<&String> for MarkerName {
    fn from(value: &String) -> Self {
        Self::parse(value)
    }
}

impl PartialEq<&str> for MarkerName {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<str> for MarkerName {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<MarkerName> for &str {
    fn eq(&self, other: &MarkerName) -> bool {
        *self == other.as_str()
    }
}

fn lookup_known_marker_exact(name: &str) -> Option<KnownMarker> {
    let info = match name {
        "id" | "usfm" | "ide" | "h" | "h1" | "h2" | "h3" | "toc1" | "toc2" | "toc3" | "toca1"
        | "toca2" | "toca3" | "mt" | "mt1" | "mt2" | "mt3" | "mt4" | "mte" | "mte1" | "mte2"
        | "imt" | "imt1" | "imt2" | "imt3" | "imt4" | "imte" | "imte1" | "imte2" | "is" | "is1"
        | "is2" | "is3" | "cl" | "cp" | "cd" => MarkerInfo::new(MarkerKind::Header),
        "p" | "m" | "po" | "pr" | "cls" | "pmo" | "pm" | "pmc" | "pmr" | "pi" | "pi1" | "pi2"
        | "pi3" | "mi" | "nb" | "pc" | "ph" | "ph1" | "ph2" | "ph3" | "b" | "pb" | "q" | "q1"
        | "q2" | "q3" | "q4" | "qr" | "qc" | "qa" | "qm" | "qm1" | "qm2" | "qm3" | "qd" | "lh"
        | "li" | "li1" | "li2" | "li3" | "li4" | "lf" | "lim" | "lim1" | "lim2" | "lim3" | "ms"
        | "ms1" | "ms2" | "ms3" | "mr" | "s" | "s1" | "s2" | "s3" | "s4" | "sr" | "r" | "sp"
        | "sd" | "sd1" | "sd2" | "sd3" | "sd4" | "d" | "ip" | "ipi" | "im" | "imi" | "ipq"
        | "imq" | "ipr" | "ib" | "iq" | "iq1" | "iq2" | "iq3" | "iex" | "iot" | "io" | "io1"
        | "io2" | "io3" | "io4" | "ili" | "ili1" | "ili2" | "ie" | "lit" => {
            MarkerInfo::new(MarkerKind::Paragraph)
        }
        "periph" => MarkerInfo::new(MarkerKind::Periph),
        "tr" => MarkerInfo::new(MarkerKind::TableRow),
        "f" | "fe" | "x" | "ef" | "ex" => MarkerInfo::new(MarkerKind::Note),
        "fr" | "ft" | "fk" | "fq" | "fqa" | "fl" | "fw" | "fp" | "fv" | "fdc" | "xop" | "xot"
        | "xnt" | "xdc" | "xo" | "xt" | "xta" | "xk" | "xq" => {
            MarkerInfo::note_sub(MarkerKind::Character)
        }
        "add" | "addpn" | "bk" | "dc" | "ior" | "iqt" | "k" | "litl" | "nd" | "ord" | "pn"
        | "png" | "qs" | "qt" | "sig" | "sls" | "tl" | "wj" | "em" | "bd" | "bdit" | "it"
        | "no" | "sc" | "sup" | "rb" | "pro" | "w" | "wg" | "wh" | "wa" | "rq" | "ca" | "va"
        | "vp" | "fm" | "jmp" | "ref" => MarkerInfo::new(MarkerKind::Character),
        "th" | "th1" | "th2" | "th3" | "tc" | "tc1" | "tc2" | "tc3" | "thr" | "thr1" | "thr2"
        | "thr3" | "tcr" | "tcr1" | "tcr2" | "tcr3" | "thc" | "thc1" | "thc2" | "thc3" | "tcc"
        | "tcc1" | "tcc2" | "tcc3" => MarkerInfo::new(MarkerKind::TableCell),
        "c" => MarkerInfo::new(MarkerKind::Chapter),
        "v" => MarkerInfo::new(MarkerKind::Verse),
        "fig" => MarkerInfo::new(MarkerKind::Figure),
        "esb" => MarkerInfo::new(MarkerKind::SidebarStart),
        "esbe" => MarkerInfo::new(MarkerKind::SidebarEnd),
        "rem" | "sts" | "restore" | "cat" => MarkerInfo::new(MarkerKind::Meta),
        "ts" => MarkerInfo::new(MarkerKind::MilestoneStart),
        _ => return None,
    };
    Some(KnownMarker::new(name, info))
}

/// Return the default attribute key for a marker, if any.
///
/// When the USFM pipe syntax uses a bare value (e.g. `\w word|grace\w*`),
/// this value is treated as the marker's default attribute. For example,
/// `\w` uses `"lemma"`, so `|grace` becomes `lemma="grace"`.
pub fn default_attribute(marker: &str) -> Option<&'static str> {
    match marker {
        "w" => Some("lemma"),
        "rb" => Some("gloss"),
        "jmp" => Some("link-href"),
        "xt" => Some("link-href"),
        "ref" => Some("loc"),
        "fig" => Some("src"),
        _ => {
            // Milestone markers: \qt-s, \qt1-s, \qt-e, etc. use "who".
            let base = marker
                .strip_suffix("-s")
                .or_else(|| marker.strip_suffix("-e"));
            if let Some(b) = base {
                let b = b.trim_end_matches(|c: char| c.is_ascii_digit());
                if b == "qt" {
                    return Some("who");
                }
            }
            None
        }
    }
}

/// Returns the list of required attribute names for a given marker.
///
/// Most markers have no required attributes; `\rb` requires `"gloss"`.
pub fn required_attributes(marker: &str) -> &'static [&'static str] {
    match marker {
        "rb" => &["gloss"],
        _ => &[],
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
        | "imte" | "imte1" | "imte2"
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
        | "b" | "pb"

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
        | "ib"
        | "iq" | "iq1" | "iq2" | "iq3"
        | "iex"

        // -- introduction outline --
        | "iot"
        | "io" | "io1" | "io2" | "io3" | "io4"

        // -- introduction lists --
        | "ili" | "ili1" | "ili2"

        // -- introduction end --
        | "ie"

        // -- liturgical note marker --
        | "lit"
        => MarkerInfo::new(MarkerKind::Paragraph),

        // =============================================================
        // Peripheral content section
        // =============================================================
        "periph" => MarkerInfo::new(MarkerKind::Periph),

        // =============================================================
        // Table row marker
        // =============================================================
        "tr" => MarkerInfo::new(MarkerKind::TableRow),

        // =============================================================
        // Note markers (footnote, endnote, cross-reference)
        // =============================================================
        "f" | "fe" | "x" | "ef" | "ex"
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
        "add" | "addpn" | "bk" | "dc" | "ior" | "iqt"
        | "k" | "litl" | "nd" | "ord"
        | "pn" | "png" | "qs" | "qt" | "sig"
        | "sls" | "tl" | "wj"

        // -- formatting --
        | "em" | "bd" | "bdit" | "it" | "no" | "sc" | "sup" | "rb"

        // -- word-level / glossary --
        | "pro" | "w" | "wg" | "wh" | "wa"

        // -- references / annotations --
        | "rq" | "ca" | "va" | "vp" | "fm"

        // -- linking / references --
        | "jmp" | "ref"

        // -- acrostic / liturgical --
        | "qac" | "lik" | "liv"

        => MarkerInfo::new(MarkerKind::Character),

        // =============================================================
        // Table cell markers (within \tr rows, implicitly close siblings)
        // =============================================================
        "th" | "th1" | "th2" | "th3"
        | "tc" | "tc1" | "tc2" | "tc3"
        | "thr" | "thr1" | "thr2" | "thr3"
        | "tcr" | "tcr1" | "tcr2" | "tcr3"
        | "thc" | "thc1" | "thc2" | "thc3"
        | "tcc" | "tcc1" | "tcc2" | "tcc3"
        => MarkerInfo::new(MarkerKind::TableCell),

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
        "rem" | "sts" | "restore" | "cat"
        => MarkerInfo::new(MarkerKind::Meta),

        // =============================================================
        // Self-closing milestone markers (use \marker\* syntax)
        // =============================================================
        "ts" => MarkerInfo::new(MarkerKind::MilestoneStart),

        // =============================================================
        // Unknown / unrecognized — with dynamic numbered-variant fallback
        // =============================================================
        _ => {
            // Strip column-spanning suffix first (e.g., "tcr1-2" -> "tcr1")
            // then strip trailing digits (e.g., "tcr1" -> "tcr").
            let without_span = if let Some(dash_pos) = name.rfind('-') {
                let after_dash = &name[dash_pos + 1..];
                if !after_dash.is_empty() && after_dash.chars().all(|c| c.is_ascii_digit()) {
                    &name[..dash_pos]
                } else {
                    name
                }
            } else {
                name
            };
            let base = without_span.trim_end_matches(|c: char| c.is_ascii_digit());
            if !base.is_empty() && base != name {
                let base_info = lookup_marker(base);
                if base_info.kind != MarkerKind::Unknown {
                    return base_info;
                }
            }
            MarkerInfo::new(MarkerKind::Unknown)
        }
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
            "id", "usfm", "ide", "h", "h1", "h2", "h3", "toc1", "toc2", "toc3", "toca1", "toca2",
            "toca3", "mt", "mt1", "mt2", "mt3", "mt4", "mte", "mte1", "mte2", "imt", "imt1",
            "imt2", "imt3", "imt4", "imte", "imte1", "imte2", "is", "is1", "is2", "is3", "cl",
            "cp", "cd",
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
            "p", "m", "po", "pr", "cls", "pmo", "pm", "pmc", "pmr", "pi", "pi1", "pi2", "pi3",
            "mi", "nb", "pc", "ph", "ph1", "ph2", "ph3", "b", "pb", "q", "q1", "q2", "q3", "q4",
            "qr", "qc", "qa", "qm", "qm1", "qm2", "qm3", "qd", "lh", "li", "li1", "li2", "li3",
            "li4", "lf", "lim", "lim1", "lim2", "lim3", "ms", "ms1", "ms2", "ms3", "mr", "s", "s1",
            "s2", "s3", "s4", "sr", "r", "sp", "sd", "sd1", "sd2", "sd3", "sd4", "d", "ip", "ipi",
            "im", "imi", "ipq", "imq", "ipr", "ib", "iq", "iq1", "iq2", "iq3", "iex", "iot", "io",
            "io1", "io2", "io3", "io4", "ili", "ili1", "ili2", "ie",
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
        // Periph is its own kind
        assert_eq!(lookup_marker("periph").kind, MarkerKind::Periph);
        // Table row is its own kind
        assert_eq!(lookup_marker("tr").kind, MarkerKind::TableRow);
    }

    // -----------------------------------------------------------------
    // Note markers
    // -----------------------------------------------------------------
    #[test]
    fn note_markers() {
        for marker in &["f", "fe", "x", "ef", "ex"] {
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
            "fr", "ft", "fk", "fq", "fqa", "fl", "fw", "fp", "fv", "fdc", "xo", "xop", "xt", "xta",
            "xk", "xq", "xot", "xnt", "xdc",
        ];
        for marker in &note_subs {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::Character,
                "expected Character for note sub-marker \\{}",
                marker,
            );
            assert!(info.valid_in_note, "\\{} should be valid_in_note", marker,);
        }
    }

    // -----------------------------------------------------------------
    // Character markers (not valid in notes)
    // -----------------------------------------------------------------
    #[test]
    fn character_markers() {
        let chars = [
            "add", "bk", "dc", "ior", "iqt", "k", "litl", "nd", "ord", "pn", "png", "qs", "qt",
            "sig", "sls", "tl", "wj", "em", "bd", "bdit", "it", "no", "sc", "sup", "rb", "pro",
            "w", "wg", "wh", "wa", "rq", "ca", "va", "vp", "jmp", "fm", "addpn",
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
    // Table cell markers
    // -----------------------------------------------------------------
    #[test]
    fn table_cell_markers() {
        let cells = [
            "th", "th1", "th2", "th3", "tc", "tc1", "tc2", "tc3", "thr", "thr1", "thr2", "thr3",
            "tcr", "tcr1", "tcr2", "tcr3", "thc", "thc1", "thc2", "thc3", "tcc", "tcc1", "tcc2",
            "tcc3",
        ];
        for marker in &cells {
            let info = lookup_marker(marker);
            assert_eq!(
                info.kind,
                MarkerKind::TableCell,
                "expected TableCell for \\{}",
                marker,
            );
        }
    }

    // -----------------------------------------------------------------
    // Dynamic numbered-variant fallback
    // -----------------------------------------------------------------
    #[test]
    fn dynamic_numbered_variants() {
        // Paragraph variants
        assert_eq!(lookup_marker("s5").kind, MarkerKind::Paragraph);
        assert_eq!(lookup_marker("ms4").kind, MarkerKind::Paragraph);
        assert_eq!(lookup_marker("ms7").kind, MarkerKind::Paragraph);
        assert_eq!(lookup_marker("q5").kind, MarkerKind::Paragraph);
        assert_eq!(lookup_marker("li5").kind, MarkerKind::Paragraph);
        assert_eq!(lookup_marker("io5").kind, MarkerKind::Paragraph);

        // TableCell variants
        assert_eq!(lookup_marker("th4").kind, MarkerKind::TableCell);
        assert_eq!(lookup_marker("tc4").kind, MarkerKind::TableCell);
        assert_eq!(lookup_marker("tc5").kind, MarkerKind::TableCell);
        assert_eq!(lookup_marker("thr4").kind, MarkerKind::TableCell);
        assert_eq!(lookup_marker("tcr4").kind, MarkerKind::TableCell);
        assert_eq!(lookup_marker("thc4").kind, MarkerKind::TableCell);
        assert_eq!(lookup_marker("tcc4").kind, MarkerKind::TableCell);

        // Liturgical note
        assert_eq!(lookup_marker("lit").kind, MarkerKind::Paragraph);

        // Genuinely unknown should stay unknown
        assert_eq!(lookup_marker("notamarker").kind, MarkerKind::Unknown);
        assert_eq!(lookup_marker("xyz99").kind, MarkerKind::Unknown);
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
        for marker in &["rem", "sts", "restore", "cat"] {
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
