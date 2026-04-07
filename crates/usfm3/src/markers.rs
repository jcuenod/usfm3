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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct KnownMarker {
    name: &'static str,
    info: MarkerInfo,
}

impl KnownMarker {
    const fn new(name: &'static str, info: MarkerInfo) -> Self {
        Self { name, info }
    }

    pub fn as_str(&self) -> &str {
        self.name
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MarkerName {
    Known(KnownMarker),
    Custom(&'static str),
}

impl MarkerName {
    pub fn parse(name: &str) -> Self {
        lookup_known_marker_exact(name)
            .map(Self::Known)
            .unwrap_or_else(|| Self::Custom(Box::leak(name.to_string().into_boxed_str())))
    }

    pub fn as_str(&self) -> &str {
        match self {
            MarkerName::Known(marker) => marker.as_str(),
            MarkerName::Custom(marker) => marker,
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
            .unwrap_or_else(|| Self::Custom(Box::leak(value.into_boxed_str())))
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
    // Returns (static_name, info) so KnownMarker stores a &'static str
    // and avoids any heap allocation.  Each arm must produce a string
    // literal so the borrow checker sees &'static str.
    let (s, info): (&'static str, MarkerInfo) = match name {
        "id" => ("id", MarkerInfo::new(MarkerKind::Header)),
        "usfm" => ("usfm", MarkerInfo::new(MarkerKind::Header)),
        "ide" => ("ide", MarkerInfo::new(MarkerKind::Header)),
        "h" => ("h", MarkerInfo::new(MarkerKind::Header)),
        "h1" => ("h1", MarkerInfo::new(MarkerKind::Header)),
        "h2" => ("h2", MarkerInfo::new(MarkerKind::Header)),
        "h3" => ("h3", MarkerInfo::new(MarkerKind::Header)),
        "toc1" => ("toc1", MarkerInfo::new(MarkerKind::Header)),
        "toc2" => ("toc2", MarkerInfo::new(MarkerKind::Header)),
        "toc3" => ("toc3", MarkerInfo::new(MarkerKind::Header)),
        "toca1" => ("toca1", MarkerInfo::new(MarkerKind::Header)),
        "toca2" => ("toca2", MarkerInfo::new(MarkerKind::Header)),
        "toca3" => ("toca3", MarkerInfo::new(MarkerKind::Header)),
        "mt" => ("mt", MarkerInfo::new(MarkerKind::Header)),
        "mt1" => ("mt1", MarkerInfo::new(MarkerKind::Header)),
        "mt2" => ("mt2", MarkerInfo::new(MarkerKind::Header)),
        "mt3" => ("mt3", MarkerInfo::new(MarkerKind::Header)),
        "mt4" => ("mt4", MarkerInfo::new(MarkerKind::Header)),
        "mte" => ("mte", MarkerInfo::new(MarkerKind::Header)),
        "mte1" => ("mte1", MarkerInfo::new(MarkerKind::Header)),
        "mte2" => ("mte2", MarkerInfo::new(MarkerKind::Header)),
        "imt" => ("imt", MarkerInfo::new(MarkerKind::Header)),
        "imt1" => ("imt1", MarkerInfo::new(MarkerKind::Header)),
        "imt2" => ("imt2", MarkerInfo::new(MarkerKind::Header)),
        "imt3" => ("imt3", MarkerInfo::new(MarkerKind::Header)),
        "imt4" => ("imt4", MarkerInfo::new(MarkerKind::Header)),
        "imte" => ("imte", MarkerInfo::new(MarkerKind::Header)),
        "imte1" => ("imte1", MarkerInfo::new(MarkerKind::Header)),
        "imte2" => ("imte2", MarkerInfo::new(MarkerKind::Header)),
        "is" => ("is", MarkerInfo::new(MarkerKind::Header)),
        "is1" => ("is1", MarkerInfo::new(MarkerKind::Header)),
        "is2" => ("is2", MarkerInfo::new(MarkerKind::Header)),
        "is3" => ("is3", MarkerInfo::new(MarkerKind::Header)),
        "cl" => ("cl", MarkerInfo::new(MarkerKind::Header)),
        "cp" => ("cp", MarkerInfo::new(MarkerKind::Header)),
        "cd" => ("cd", MarkerInfo::new(MarkerKind::Header)),
        "p" => ("p", MarkerInfo::new(MarkerKind::Paragraph)),
        "m" => ("m", MarkerInfo::new(MarkerKind::Paragraph)),
        "po" => ("po", MarkerInfo::new(MarkerKind::Paragraph)),
        "pr" => ("pr", MarkerInfo::new(MarkerKind::Paragraph)),
        "cls" => ("cls", MarkerInfo::new(MarkerKind::Paragraph)),
        "pmo" => ("pmo", MarkerInfo::new(MarkerKind::Paragraph)),
        "pm" => ("pm", MarkerInfo::new(MarkerKind::Paragraph)),
        "pmc" => ("pmc", MarkerInfo::new(MarkerKind::Paragraph)),
        "pmr" => ("pmr", MarkerInfo::new(MarkerKind::Paragraph)),
        "pi" => ("pi", MarkerInfo::new(MarkerKind::Paragraph)),
        "pi1" => ("pi1", MarkerInfo::new(MarkerKind::Paragraph)),
        "pi2" => ("pi2", MarkerInfo::new(MarkerKind::Paragraph)),
        "pi3" => ("pi3", MarkerInfo::new(MarkerKind::Paragraph)),
        "mi" => ("mi", MarkerInfo::new(MarkerKind::Paragraph)),
        "nb" => ("nb", MarkerInfo::new(MarkerKind::Paragraph)),
        "pc" => ("pc", MarkerInfo::new(MarkerKind::Paragraph)),
        "ph" => ("ph", MarkerInfo::new(MarkerKind::Paragraph)),
        "ph1" => ("ph1", MarkerInfo::new(MarkerKind::Paragraph)),
        "ph2" => ("ph2", MarkerInfo::new(MarkerKind::Paragraph)),
        "ph3" => ("ph3", MarkerInfo::new(MarkerKind::Paragraph)),
        "b" => ("b", MarkerInfo::new(MarkerKind::Paragraph)),
        "pb" => ("pb", MarkerInfo::new(MarkerKind::Paragraph)),
        "q" => ("q", MarkerInfo::new(MarkerKind::Paragraph)),
        "q1" => ("q1", MarkerInfo::new(MarkerKind::Paragraph)),
        "q2" => ("q2", MarkerInfo::new(MarkerKind::Paragraph)),
        "q3" => ("q3", MarkerInfo::new(MarkerKind::Paragraph)),
        "q4" => ("q4", MarkerInfo::new(MarkerKind::Paragraph)),
        "qr" => ("qr", MarkerInfo::new(MarkerKind::Paragraph)),
        "qc" => ("qc", MarkerInfo::new(MarkerKind::Paragraph)),
        "qa" => ("qa", MarkerInfo::new(MarkerKind::Paragraph)),
        "qm" => ("qm", MarkerInfo::new(MarkerKind::Paragraph)),
        "qm1" => ("qm1", MarkerInfo::new(MarkerKind::Paragraph)),
        "qm2" => ("qm2", MarkerInfo::new(MarkerKind::Paragraph)),
        "qm3" => ("qm3", MarkerInfo::new(MarkerKind::Paragraph)),
        "qd" => ("qd", MarkerInfo::new(MarkerKind::Paragraph)),
        "lh" => ("lh", MarkerInfo::new(MarkerKind::Paragraph)),
        "li" => ("li", MarkerInfo::new(MarkerKind::Paragraph)),
        "li1" => ("li1", MarkerInfo::new(MarkerKind::Paragraph)),
        "li2" => ("li2", MarkerInfo::new(MarkerKind::Paragraph)),
        "li3" => ("li3", MarkerInfo::new(MarkerKind::Paragraph)),
        "li4" => ("li4", MarkerInfo::new(MarkerKind::Paragraph)),
        "lf" => ("lf", MarkerInfo::new(MarkerKind::Paragraph)),
        "lim" => ("lim", MarkerInfo::new(MarkerKind::Paragraph)),
        "lim1" => ("lim1", MarkerInfo::new(MarkerKind::Paragraph)),
        "lim2" => ("lim2", MarkerInfo::new(MarkerKind::Paragraph)),
        "lim3" => ("lim3", MarkerInfo::new(MarkerKind::Paragraph)),
        "ms" => ("ms", MarkerInfo::new(MarkerKind::Paragraph)),
        "ms1" => ("ms1", MarkerInfo::new(MarkerKind::Paragraph)),
        "ms2" => ("ms2", MarkerInfo::new(MarkerKind::Paragraph)),
        "ms3" => ("ms3", MarkerInfo::new(MarkerKind::Paragraph)),
        "mr" => ("mr", MarkerInfo::new(MarkerKind::Paragraph)),
        "s" => ("s", MarkerInfo::new(MarkerKind::Paragraph)),
        "s1" => ("s1", MarkerInfo::new(MarkerKind::Paragraph)),
        "s2" => ("s2", MarkerInfo::new(MarkerKind::Paragraph)),
        "s3" => ("s3", MarkerInfo::new(MarkerKind::Paragraph)),
        "s4" => ("s4", MarkerInfo::new(MarkerKind::Paragraph)),
        "sr" => ("sr", MarkerInfo::new(MarkerKind::Paragraph)),
        "r" => ("r", MarkerInfo::new(MarkerKind::Paragraph)),
        "sp" => ("sp", MarkerInfo::new(MarkerKind::Paragraph)),
        "sd" => ("sd", MarkerInfo::new(MarkerKind::Paragraph)),
        "sd1" => ("sd1", MarkerInfo::new(MarkerKind::Paragraph)),
        "sd2" => ("sd2", MarkerInfo::new(MarkerKind::Paragraph)),
        "sd3" => ("sd3", MarkerInfo::new(MarkerKind::Paragraph)),
        "sd4" => ("sd4", MarkerInfo::new(MarkerKind::Paragraph)),
        "d" => ("d", MarkerInfo::new(MarkerKind::Paragraph)),
        "ip" => ("ip", MarkerInfo::new(MarkerKind::Paragraph)),
        "ipi" => ("ipi", MarkerInfo::new(MarkerKind::Paragraph)),
        "im" => ("im", MarkerInfo::new(MarkerKind::Paragraph)),
        "imi" => ("imi", MarkerInfo::new(MarkerKind::Paragraph)),
        "ipq" => ("ipq", MarkerInfo::new(MarkerKind::Paragraph)),
        "imq" => ("imq", MarkerInfo::new(MarkerKind::Paragraph)),
        "ipr" => ("ipr", MarkerInfo::new(MarkerKind::Paragraph)),
        "ib" => ("ib", MarkerInfo::new(MarkerKind::Paragraph)),
        "iq" => ("iq", MarkerInfo::new(MarkerKind::Paragraph)),
        "iq1" => ("iq1", MarkerInfo::new(MarkerKind::Paragraph)),
        "iq2" => ("iq2", MarkerInfo::new(MarkerKind::Paragraph)),
        "iq3" => ("iq3", MarkerInfo::new(MarkerKind::Paragraph)),
        "iex" => ("iex", MarkerInfo::new(MarkerKind::Paragraph)),
        "iot" => ("iot", MarkerInfo::new(MarkerKind::Paragraph)),
        "io" => ("io", MarkerInfo::new(MarkerKind::Paragraph)),
        "io1" => ("io1", MarkerInfo::new(MarkerKind::Paragraph)),
        "io2" => ("io2", MarkerInfo::new(MarkerKind::Paragraph)),
        "io3" => ("io3", MarkerInfo::new(MarkerKind::Paragraph)),
        "io4" => ("io4", MarkerInfo::new(MarkerKind::Paragraph)),
        "ili" => ("ili", MarkerInfo::new(MarkerKind::Paragraph)),
        "ili1" => ("ili1", MarkerInfo::new(MarkerKind::Paragraph)),
        "ili2" => ("ili2", MarkerInfo::new(MarkerKind::Paragraph)),
        "ie" => ("ie", MarkerInfo::new(MarkerKind::Paragraph)),
        "lit" => ("lit", MarkerInfo::new(MarkerKind::Paragraph)),
        "periph" => ("periph", MarkerInfo::new(MarkerKind::Periph)),
        "tr" => ("tr", MarkerInfo::new(MarkerKind::TableRow)),
        "f" => ("f", MarkerInfo::new(MarkerKind::Note)),
        "fe" => ("fe", MarkerInfo::new(MarkerKind::Note)),
        "x" => ("x", MarkerInfo::new(MarkerKind::Note)),
        "ef" => ("ef", MarkerInfo::new(MarkerKind::Note)),
        "ex" => ("ex", MarkerInfo::new(MarkerKind::Note)),
        "fr" => ("fr", MarkerInfo::note_sub(MarkerKind::Character)),
        "ft" => ("ft", MarkerInfo::note_sub(MarkerKind::Character)),
        "fk" => ("fk", MarkerInfo::note_sub(MarkerKind::Character)),
        "fq" => ("fq", MarkerInfo::note_sub(MarkerKind::Character)),
        "fqa" => ("fqa", MarkerInfo::note_sub(MarkerKind::Character)),
        "fl" => ("fl", MarkerInfo::note_sub(MarkerKind::Character)),
        "fw" => ("fw", MarkerInfo::note_sub(MarkerKind::Character)),
        "fp" => ("fp", MarkerInfo::note_sub(MarkerKind::Character)),
        "fv" => ("fv", MarkerInfo::note_sub(MarkerKind::Character)),
        "fdc" => ("fdc", MarkerInfo::note_sub(MarkerKind::Character)),
        "xop" => ("xop", MarkerInfo::note_sub(MarkerKind::Character)),
        "xot" => ("xot", MarkerInfo::note_sub(MarkerKind::Character)),
        "xnt" => ("xnt", MarkerInfo::note_sub(MarkerKind::Character)),
        "xdc" => ("xdc", MarkerInfo::note_sub(MarkerKind::Character)),
        "xo" => ("xo", MarkerInfo::note_sub(MarkerKind::Character)),
        "xt" => ("xt", MarkerInfo::note_sub(MarkerKind::Character)),
        "xta" => ("xta", MarkerInfo::note_sub(MarkerKind::Character)),
        "xk" => ("xk", MarkerInfo::note_sub(MarkerKind::Character)),
        "xq" => ("xq", MarkerInfo::note_sub(MarkerKind::Character)),
        "add" => ("add", MarkerInfo::new(MarkerKind::Character)),
        "addpn" => ("addpn", MarkerInfo::new(MarkerKind::Character)),
        "bk" => ("bk", MarkerInfo::new(MarkerKind::Character)),
        "dc" => ("dc", MarkerInfo::new(MarkerKind::Character)),
        "ior" => ("ior", MarkerInfo::new(MarkerKind::Character)),
        "iqt" => ("iqt", MarkerInfo::new(MarkerKind::Character)),
        "k" => ("k", MarkerInfo::new(MarkerKind::Character)),
        "litl" => ("litl", MarkerInfo::new(MarkerKind::Character)),
        "nd" => ("nd", MarkerInfo::new(MarkerKind::Character)),
        "ord" => ("ord", MarkerInfo::new(MarkerKind::Character)),
        "pn" => ("pn", MarkerInfo::new(MarkerKind::Character)),
        "png" => ("png", MarkerInfo::new(MarkerKind::Character)),
        "qs" => ("qs", MarkerInfo::new(MarkerKind::Character)),
        "qt" => ("qt", MarkerInfo::new(MarkerKind::Character)),
        "sig" => ("sig", MarkerInfo::new(MarkerKind::Character)),
        "sls" => ("sls", MarkerInfo::new(MarkerKind::Character)),
        "tl" => ("tl", MarkerInfo::new(MarkerKind::Character)),
        "wj" => ("wj", MarkerInfo::new(MarkerKind::Character)),
        "em" => ("em", MarkerInfo::new(MarkerKind::Character)),
        "bd" => ("bd", MarkerInfo::new(MarkerKind::Character)),
        "bdit" => ("bdit", MarkerInfo::new(MarkerKind::Character)),
        "it" => ("it", MarkerInfo::new(MarkerKind::Character)),
        "no" => ("no", MarkerInfo::new(MarkerKind::Character)),
        "sc" => ("sc", MarkerInfo::new(MarkerKind::Character)),
        "sup" => ("sup", MarkerInfo::new(MarkerKind::Character)),
        "rb" => ("rb", MarkerInfo::new(MarkerKind::Character)),
        "pro" => ("pro", MarkerInfo::new(MarkerKind::Character)),
        "w" => ("w", MarkerInfo::new(MarkerKind::Character)),
        "wg" => ("wg", MarkerInfo::new(MarkerKind::Character)),
        "wh" => ("wh", MarkerInfo::new(MarkerKind::Character)),
        "wa" => ("wa", MarkerInfo::new(MarkerKind::Character)),
        "rq" => ("rq", MarkerInfo::new(MarkerKind::Character)),
        "ca" => ("ca", MarkerInfo::new(MarkerKind::Character)),
        "va" => ("va", MarkerInfo::new(MarkerKind::Character)),
        "vp" => ("vp", MarkerInfo::new(MarkerKind::Character)),
        "fm" => ("fm", MarkerInfo::new(MarkerKind::Character)),
        "jmp" => ("jmp", MarkerInfo::new(MarkerKind::Character)),
        "ref" => ("ref", MarkerInfo::new(MarkerKind::Character)),
        "th" => ("th", MarkerInfo::new(MarkerKind::TableCell)),
        "th1" => ("th1", MarkerInfo::new(MarkerKind::TableCell)),
        "th2" => ("th2", MarkerInfo::new(MarkerKind::TableCell)),
        "th3" => ("th3", MarkerInfo::new(MarkerKind::TableCell)),
        "tc" => ("tc", MarkerInfo::new(MarkerKind::TableCell)),
        "tc1" => ("tc1", MarkerInfo::new(MarkerKind::TableCell)),
        "tc2" => ("tc2", MarkerInfo::new(MarkerKind::TableCell)),
        "tc3" => ("tc3", MarkerInfo::new(MarkerKind::TableCell)),
        "thr" => ("thr", MarkerInfo::new(MarkerKind::TableCell)),
        "thr1" => ("thr1", MarkerInfo::new(MarkerKind::TableCell)),
        "thr2" => ("thr2", MarkerInfo::new(MarkerKind::TableCell)),
        "thr3" => ("thr3", MarkerInfo::new(MarkerKind::TableCell)),
        "tcr" => ("tcr", MarkerInfo::new(MarkerKind::TableCell)),
        "tcr1" => ("tcr1", MarkerInfo::new(MarkerKind::TableCell)),
        "tcr2" => ("tcr2", MarkerInfo::new(MarkerKind::TableCell)),
        "tcr3" => ("tcr3", MarkerInfo::new(MarkerKind::TableCell)),
        "thc" => ("thc", MarkerInfo::new(MarkerKind::TableCell)),
        "thc1" => ("thc1", MarkerInfo::new(MarkerKind::TableCell)),
        "thc2" => ("thc2", MarkerInfo::new(MarkerKind::TableCell)),
        "thc3" => ("thc3", MarkerInfo::new(MarkerKind::TableCell)),
        "tcc" => ("tcc", MarkerInfo::new(MarkerKind::TableCell)),
        "tcc1" => ("tcc1", MarkerInfo::new(MarkerKind::TableCell)),
        "tcc2" => ("tcc2", MarkerInfo::new(MarkerKind::TableCell)),
        "tcc3" => ("tcc3", MarkerInfo::new(MarkerKind::TableCell)),
        "c" => ("c", MarkerInfo::new(MarkerKind::Chapter)),
        "v" => ("v", MarkerInfo::new(MarkerKind::Verse)),
        "fig" => ("fig", MarkerInfo::new(MarkerKind::Figure)),
        "esb" => ("esb", MarkerInfo::new(MarkerKind::SidebarStart)),
        "esbe" => ("esbe", MarkerInfo::new(MarkerKind::SidebarEnd)),
        "rem" => ("rem", MarkerInfo::new(MarkerKind::Meta)),
        "sts" => ("sts", MarkerInfo::new(MarkerKind::Meta)),
        "restore" => ("restore", MarkerInfo::new(MarkerKind::Meta)),
        "cat" => ("cat", MarkerInfo::new(MarkerKind::Meta)),
        "ts" => ("ts", MarkerInfo::new(MarkerKind::MilestoneStart)),
        _ => return None,
    };
    Some(KnownMarker::new(s, info))
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
