use std::collections::BTreeSet;

use usfm3::diagnostics::DiagnosticCode;
use usfm3::markers::{MarkerKind, MarkerName, lookup_marker};

#[derive(Clone, Copy)]
struct ExactMarkerSpec {
    family: &'static str,
    marker: &'static str,
    kind: MarkerKind,
}

#[derive(Clone, Copy)]
struct PatternMarkerSpec {
    family: &'static str,
    notation: &'static str,
    kind: MarkerKind,
    samples: &'static [&'static str],
}

#[derive(Clone, Copy)]
struct SourceInventory {
    name: &'static str,
    exact: &'static [ExactMarkerSpec],
    patterns: &'static [PatternMarkerSpec],
}

const DOCS_USFM_BIBLE_31_EXACT: &[ExactMarkerSpec] = &[
    ExactMarkerSpec {
        family: "id",
        marker: "id",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "usfm",
        marker: "usfm",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "c",
        marker: "c",
        kind: MarkerKind::Chapter,
    },
    ExactMarkerSpec {
        family: "ca",
        marker: "ca",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "cp",
        marker: "cp",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "v",
        marker: "v",
        kind: MarkerKind::Verse,
    },
    ExactMarkerSpec {
        family: "va",
        marker: "va",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "vp",
        marker: "vp",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ide",
        marker: "ide",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "sts",
        marker: "sts",
        kind: MarkerKind::Meta,
    },
    ExactMarkerSpec {
        family: "rem",
        marker: "rem",
        kind: MarkerKind::Meta,
    },
    ExactMarkerSpec {
        family: "h",
        marker: "h",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "ip",
        marker: "ip",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ipi",
        marker: "ipi",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "im",
        marker: "im",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "imi",
        marker: "imi",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ipq",
        marker: "ipq",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "imq",
        marker: "imq",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ipr",
        marker: "ipr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ib",
        marker: "ib",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "iot",
        marker: "iot",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "iex",
        marker: "iex",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "imte",
        marker: "imte",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "ie",
        marker: "ie",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "cl",
        marker: "cl",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "cd",
        marker: "cd",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "mr",
        marker: "mr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "sr",
        marker: "sr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "r",
        marker: "r",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "d",
        marker: "d",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "sp",
        marker: "sp",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "p",
        marker: "p",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "m",
        marker: "m",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "po",
        marker: "po",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "cls",
        marker: "cls",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pr",
        marker: "pr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pc",
        marker: "pc",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pm",
        marker: "pm",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pmo",
        marker: "pmo",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pmc",
        marker: "pmc",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pmr",
        marker: "pmr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "lit",
        marker: "lit",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "nb",
        marker: "nb",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "b",
        marker: "b",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ph",
        marker: "ph",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "qr",
        marker: "qr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "qc",
        marker: "qc",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "qa",
        marker: "qa",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "qd",
        marker: "qd",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "lh",
        marker: "lh",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "lf",
        marker: "lf",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "tr",
        marker: "tr",
        kind: MarkerKind::TableRow,
    },
    ExactMarkerSpec {
        family: "add",
        marker: "add",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "bk",
        marker: "bk",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "dc",
        marker: "dc",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "em",
        marker: "em",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "jmp",
        marker: "jmp",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "k",
        marker: "k",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "nd",
        marker: "nd",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ord",
        marker: "ord",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "pn",
        marker: "pn",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "png",
        marker: "png",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "qt",
        marker: "qt",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "rb",
        marker: "rb",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "rq",
        marker: "rq",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ref",
        marker: "ref",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "sig",
        marker: "sig",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "sls",
        marker: "sls",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "tl",
        marker: "tl",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "w",
        marker: "w",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "wa",
        marker: "wa",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "wg",
        marker: "wg",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "wh",
        marker: "wh",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "wj",
        marker: "wj",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "addpn",
        marker: "addpn",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "pro",
        marker: "pro",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "bd",
        marker: "bd",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "it",
        marker: "it",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "bdit",
        marker: "bdit",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "no",
        marker: "no",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "sc",
        marker: "sc",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "sup",
        marker: "sup",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "pb",
        marker: "pb",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ior",
        marker: "ior",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "iqt",
        marker: "iqt",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "qac",
        marker: "qac",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "qs",
        marker: "qs",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "litl",
        marker: "litl",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "lik",
        marker: "lik",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "liv",
        marker: "liv",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fr",
        marker: "fr",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fq",
        marker: "fq",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fqa",
        marker: "fqa",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fk",
        marker: "fk",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ft",
        marker: "ft",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fl",
        marker: "fl",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fw",
        marker: "fw",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fp",
        marker: "fp",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fv",
        marker: "fv",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fdc",
        marker: "fdc",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fm",
        marker: "fm",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xo",
        marker: "xo",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xop",
        marker: "xop",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xk",
        marker: "xk",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xq",
        marker: "xq",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xt",
        marker: "xt",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xta",
        marker: "xta",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xot",
        marker: "xot",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xnt",
        marker: "xnt",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xdc",
        marker: "xdc",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "f",
        marker: "f",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "fe",
        marker: "fe",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "ef",
        marker: "ef",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "efe",
        marker: "efe",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "x",
        marker: "x",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "ex",
        marker: "ex",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "esb",
        marker: "esb",
        kind: MarkerKind::SidebarStart,
    },
    ExactMarkerSpec {
        family: "cat",
        marker: "cat",
        kind: MarkerKind::Meta,
    },
    ExactMarkerSpec {
        family: "fig",
        marker: "fig",
        kind: MarkerKind::Figure,
    },
    ExactMarkerSpec {
        family: "periph",
        marker: "periph",
        kind: MarkerKind::Periph,
    },
    ExactMarkerSpec {
        family: "ts",
        marker: "ts",
        kind: MarkerKind::MilestoneStart,
    },
];

const DOCS_USFM_BIBLE_31_PATTERNS: &[PatternMarkerSpec] = &[
    PatternMarkerSpec {
        family: "toc",
        notation: "toc#",
        kind: MarkerKind::Header,
        samples: &["toc1", "toc2", "toc3"],
    },
    PatternMarkerSpec {
        family: "toca",
        notation: "toca#",
        kind: MarkerKind::Header,
        samples: &["toca1", "toca2", "toca3"],
    },
    PatternMarkerSpec {
        family: "imt",
        notation: "imt#",
        kind: MarkerKind::Header,
        samples: &["imt1", "imt2", "imt4"],
    },
    PatternMarkerSpec {
        family: "is",
        notation: "is#",
        kind: MarkerKind::Header,
        samples: &["is1", "is2", "is3"],
    },
    PatternMarkerSpec {
        family: "iq",
        notation: "iq#",
        kind: MarkerKind::Paragraph,
        samples: &["iq1", "iq2", "iq3"],
    },
    PatternMarkerSpec {
        family: "ili",
        notation: "ili#",
        kind: MarkerKind::Paragraph,
        samples: &["ili1", "ili2"],
    },
    PatternMarkerSpec {
        family: "io",
        notation: "io#",
        kind: MarkerKind::Paragraph,
        samples: &["io1", "io2", "io4"],
    },
    PatternMarkerSpec {
        family: "mt",
        notation: "mt#",
        kind: MarkerKind::Header,
        samples: &["mt1", "mt2", "mt4"],
    },
    PatternMarkerSpec {
        family: "mte",
        notation: "mte#",
        kind: MarkerKind::Header,
        samples: &["mte1", "mte2"],
    },
    PatternMarkerSpec {
        family: "ms",
        notation: "ms#",
        kind: MarkerKind::Paragraph,
        samples: &["ms1", "ms2", "ms4"],
    },
    PatternMarkerSpec {
        family: "s",
        notation: "s#",
        kind: MarkerKind::Paragraph,
        samples: &["s1", "s2", "s5"],
    },
    PatternMarkerSpec {
        family: "sd",
        notation: "sd#",
        kind: MarkerKind::Paragraph,
        samples: &["sd1", "sd2", "sd5"],
    },
    PatternMarkerSpec {
        family: "pi",
        notation: "pi#",
        kind: MarkerKind::Paragraph,
        samples: &["pi1", "pi2", "pi4"],
    },
    PatternMarkerSpec {
        family: "mi",
        notation: "mi#",
        kind: MarkerKind::Paragraph,
        samples: &["mi1", "mi2"],
    },
    PatternMarkerSpec {
        family: "q",
        notation: "q#",
        kind: MarkerKind::Paragraph,
        samples: &["q1", "q2", "q5"],
    },
    PatternMarkerSpec {
        family: "qm",
        notation: "qm#",
        kind: MarkerKind::Paragraph,
        samples: &["qm1", "qm2", "qm4"],
    },
    PatternMarkerSpec {
        family: "li",
        notation: "li#",
        kind: MarkerKind::Paragraph,
        samples: &["li1", "li2", "li5"],
    },
    PatternMarkerSpec {
        family: "lim",
        notation: "lim#",
        kind: MarkerKind::Paragraph,
        samples: &["lim1", "lim2", "lim4"],
    },
    PatternMarkerSpec {
        family: "th",
        notation: "th#",
        kind: MarkerKind::TableCell,
        samples: &["th1", "th2", "th4"],
    },
    PatternMarkerSpec {
        family: "thr",
        notation: "thr#",
        kind: MarkerKind::TableCell,
        samples: &["thr1", "thr2", "thr4"],
    },
    PatternMarkerSpec {
        family: "thc",
        notation: "thc#",
        kind: MarkerKind::TableCell,
        samples: &["thc1", "thc2", "thc4"],
    },
    PatternMarkerSpec {
        family: "tc",
        notation: "tc#",
        kind: MarkerKind::TableCell,
        samples: &["tc1", "tc2", "tc4"],
    },
    PatternMarkerSpec {
        family: "tcr",
        notation: "tcr#",
        kind: MarkerKind::TableCell,
        samples: &["tcr1", "tcr2", "tcr4"],
    },
    PatternMarkerSpec {
        family: "tcc",
        notation: "tcc#",
        kind: MarkerKind::TableCell,
        samples: &["tcc1", "tcc2", "tcc4"],
    },
    PatternMarkerSpec {
        family: "qt",
        notation: "qt#",
        kind: MarkerKind::MilestoneStart,
        samples: &["qt1-s", "qt4-s"],
    },
];

const UBSICAP_30_EXACT: &[ExactMarkerSpec] = &[
    ExactMarkerSpec {
        family: "id",
        marker: "id",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "usfm",
        marker: "usfm",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "ide",
        marker: "ide",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "sts",
        marker: "sts",
        kind: MarkerKind::Meta,
    },
    ExactMarkerSpec {
        family: "rem",
        marker: "rem",
        kind: MarkerKind::Meta,
    },
    ExactMarkerSpec {
        family: "h",
        marker: "h",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "cl",
        marker: "cl",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "cd",
        marker: "cd",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "c",
        marker: "c",
        kind: MarkerKind::Chapter,
    },
    ExactMarkerSpec {
        family: "ca",
        marker: "ca",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "cp",
        marker: "cp",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "v",
        marker: "v",
        kind: MarkerKind::Verse,
    },
    ExactMarkerSpec {
        family: "va",
        marker: "va",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "vp",
        marker: "vp",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ip",
        marker: "ip",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ipi",
        marker: "ipi",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "im",
        marker: "im",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "imi",
        marker: "imi",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "imq",
        marker: "imq",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ib",
        marker: "ib",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ie",
        marker: "ie",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "iex",
        marker: "iex",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "iot",
        marker: "iot",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ipq",
        marker: "ipq",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "ipr",
        marker: "ipr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "mt",
        marker: "mt1",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "mte",
        marker: "mte1",
        kind: MarkerKind::Header,
    },
    ExactMarkerSpec {
        family: "mr",
        marker: "mr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "r",
        marker: "r",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "sr",
        marker: "sr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "sp",
        marker: "sp",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "d",
        marker: "d",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "p",
        marker: "p",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "cls",
        marker: "cls",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "m",
        marker: "m",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "mi",
        marker: "mi",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "po",
        marker: "po",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pm",
        marker: "pm",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pmo",
        marker: "pmo",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pmc",
        marker: "pmc",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pmr",
        marker: "pmr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pr",
        marker: "pr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pc",
        marker: "pc",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "nb",
        marker: "nb",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "b",
        marker: "b",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "pb",
        marker: "pb",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "qa",
        marker: "qa",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "qc",
        marker: "qc",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "qr",
        marker: "qr",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "qd",
        marker: "qd",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "lh",
        marker: "lh",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "lf",
        marker: "lf",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "lit",
        marker: "lit",
        kind: MarkerKind::Paragraph,
    },
    ExactMarkerSpec {
        family: "tr",
        marker: "tr",
        kind: MarkerKind::TableRow,
    },
    ExactMarkerSpec {
        family: "add",
        marker: "add",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "addpn",
        marker: "addpn",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "bk",
        marker: "bk",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "dc",
        marker: "dc",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "em",
        marker: "em",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "bd",
        marker: "bd",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "bdit",
        marker: "bdit",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "it",
        marker: "it",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ior",
        marker: "ior",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "iqt",
        marker: "iqt",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "jmp",
        marker: "jmp",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "k",
        marker: "k",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "litl",
        marker: "litl",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "lik",
        marker: "lik",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "nd",
        marker: "nd",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ndx",
        marker: "ndx",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "no",
        marker: "no",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ord",
        marker: "ord",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "pn",
        marker: "pn",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "png",
        marker: "png",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "pro",
        marker: "pro",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "qac",
        marker: "qac",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "qs",
        marker: "qs",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "qt",
        marker: "qt",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "rb",
        marker: "rb",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ref",
        marker: "ref",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "rq",
        marker: "rq",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "sc",
        marker: "sc",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "sig",
        marker: "sig",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "sls",
        marker: "sls",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "sup",
        marker: "sup",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "tl",
        marker: "tl",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "w",
        marker: "w",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "wa",
        marker: "wa",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "wg",
        marker: "wg",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "wh",
        marker: "wh",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "wj",
        marker: "wj",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fr",
        marker: "fr",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fk",
        marker: "fk",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fl",
        marker: "fl",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fm",
        marker: "fm",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fp",
        marker: "fp",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fq",
        marker: "fq",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fqa",
        marker: "fqa",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "ft",
        marker: "ft",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fv",
        marker: "fv",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fw",
        marker: "fw",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "fdc",
        marker: "fdc",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xo",
        marker: "xo",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xop",
        marker: "xop",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xk",
        marker: "xk",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xq",
        marker: "xq",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xt",
        marker: "xt",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xta",
        marker: "xta",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xot",
        marker: "xot",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xnt",
        marker: "xnt",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "xdc",
        marker: "xdc",
        kind: MarkerKind::Character,
    },
    ExactMarkerSpec {
        family: "f",
        marker: "f",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "fe",
        marker: "fe",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "ef",
        marker: "ef",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "x",
        marker: "x",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "ex",
        marker: "ex",
        kind: MarkerKind::Note,
    },
    ExactMarkerSpec {
        family: "esb",
        marker: "esb",
        kind: MarkerKind::SidebarStart,
    },
    ExactMarkerSpec {
        family: "esb",
        marker: "esbe",
        kind: MarkerKind::SidebarEnd,
    },
    ExactMarkerSpec {
        family: "cat",
        marker: "cat",
        kind: MarkerKind::Meta,
    },
    ExactMarkerSpec {
        family: "fig",
        marker: "fig",
        kind: MarkerKind::Figure,
    },
    ExactMarkerSpec {
        family: "periph",
        marker: "periph",
        kind: MarkerKind::Periph,
    },
];

const UBSICAP_30_PATTERNS: &[PatternMarkerSpec] = &[
    PatternMarkerSpec {
        family: "toc",
        notation: "toc#",
        kind: MarkerKind::Header,
        samples: &["toc1", "toc2", "toc3"],
    },
    PatternMarkerSpec {
        family: "toca",
        notation: "toca#",
        kind: MarkerKind::Header,
        samples: &["toca1", "toca2", "toca3"],
    },
    PatternMarkerSpec {
        family: "imt",
        notation: "imt#",
        kind: MarkerKind::Header,
        samples: &["imt1", "imt2", "imt4"],
    },
    PatternMarkerSpec {
        family: "imte",
        notation: "imte#",
        kind: MarkerKind::Header,
        samples: &["imte1", "imte2"],
    },
    PatternMarkerSpec {
        family: "is",
        notation: "is#",
        kind: MarkerKind::Header,
        samples: &["is1", "is2", "is3"],
    },
    PatternMarkerSpec {
        family: "ili",
        notation: "ili#",
        kind: MarkerKind::Paragraph,
        samples: &["ili1", "ili2"],
    },
    PatternMarkerSpec {
        family: "io",
        notation: "io#",
        kind: MarkerKind::Paragraph,
        samples: &["io1", "io2", "io4"],
    },
    PatternMarkerSpec {
        family: "iq",
        notation: "iq#",
        kind: MarkerKind::Paragraph,
        samples: &["iq1", "iq2", "iq3"],
    },
    PatternMarkerSpec {
        family: "li",
        notation: "li#",
        kind: MarkerKind::Paragraph,
        samples: &["li1", "li2", "li5"],
    },
    PatternMarkerSpec {
        family: "lim",
        notation: "lim#",
        kind: MarkerKind::Paragraph,
        samples: &["lim1", "lim2", "lim4"],
    },
    PatternMarkerSpec {
        family: "liv",
        notation: "liv#",
        kind: MarkerKind::Character,
        samples: &["liv1", "liv2", "liv4"],
    },
    PatternMarkerSpec {
        family: "ms",
        notation: "ms#",
        kind: MarkerKind::Paragraph,
        samples: &["ms1", "ms2", "ms4"],
    },
    PatternMarkerSpec {
        family: "ph",
        notation: "ph#",
        kind: MarkerKind::Paragraph,
        samples: &["ph1", "ph2", "ph4"],
    },
    PatternMarkerSpec {
        family: "pi",
        notation: "pi#",
        kind: MarkerKind::Paragraph,
        samples: &["pi1", "pi2", "pi4"],
    },
    PatternMarkerSpec {
        family: "q",
        notation: "q#",
        kind: MarkerKind::Paragraph,
        samples: &["q1", "q2", "q5"],
    },
    PatternMarkerSpec {
        family: "qm",
        notation: "qm#",
        kind: MarkerKind::Paragraph,
        samples: &["qm1", "qm2", "qm4"],
    },
    PatternMarkerSpec {
        family: "qt",
        notation: "qt#-s/qt#-e",
        kind: MarkerKind::MilestoneStart,
        samples: &["qt1-s", "qt4-s"],
    },
    PatternMarkerSpec {
        family: "s",
        notation: "s#",
        kind: MarkerKind::Paragraph,
        samples: &["s1", "s2", "s5"],
    },
    PatternMarkerSpec {
        family: "sd",
        notation: "sd#",
        kind: MarkerKind::Paragraph,
        samples: &["sd1", "sd2", "sd5"],
    },
    PatternMarkerSpec {
        family: "tc",
        notation: "tc#",
        kind: MarkerKind::TableCell,
        samples: &["tc1", "tc2", "tc4"],
    },
    PatternMarkerSpec {
        family: "tcr",
        notation: "tcr#",
        kind: MarkerKind::TableCell,
        samples: &["tcr1", "tcr2", "tcr4"],
    },
    PatternMarkerSpec {
        family: "th",
        notation: "th#",
        kind: MarkerKind::TableCell,
        samples: &["th1", "th2", "th4"],
    },
    PatternMarkerSpec {
        family: "thr",
        notation: "thr#",
        kind: MarkerKind::TableCell,
        samples: &["thr1", "thr2", "thr4"],
    },
    PatternMarkerSpec {
        family: "ts",
        notation: "ts-s/ts-e",
        kind: MarkerKind::MilestoneStart,
        samples: &["ts-s"],
    },
];

const DOCS_USFM_BIBLE_31: SourceInventory = SourceInventory {
    name: "docs.usfm.bible 3.1",
    exact: DOCS_USFM_BIBLE_31_EXACT,
    patterns: DOCS_USFM_BIBLE_31_PATTERNS,
};

const UBSICAP_30: SourceInventory = SourceInventory {
    name: "ubsicap.github.io/usfm 3.0",
    exact: UBSICAP_30_EXACT,
    patterns: UBSICAP_30_PATTERNS,
};

fn source_families(source: SourceInventory) -> BTreeSet<&'static str> {
    let mut families = BTreeSet::new();
    for marker in source.exact {
        families.insert(marker.family);
    }
    for pattern in source.patterns {
        families.insert(pattern.family);
    }
    families
}

fn assert_registry_match(marker: &str, expected_kind: MarkerKind, context: &str) {
    let actual = lookup_marker(marker).kind;
    assert_eq!(
        actual, expected_kind,
        "{context}: expected {expected_kind:?} for \\{marker}, got {actual:?}"
    );
}

fn assert_exact_marker_is_known(marker: &str, context: &str) {
    assert!(
        matches!(MarkerName::parse(marker), MarkerName::Known(_)),
        "{context}: expected \\{marker} to be in the exact known-marker table"
    );
}

fn parse_has_no_unknown_marker(marker: &str, kind: MarkerKind) {
    let snippet = snippet_for_marker(marker, kind);
    let parsed = usfm3::parse(&snippet, usfm3::ParseOptions { diagnostics: true });
    let diagnostics = parsed.diagnostics().unwrap_or(&[]);
    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.code == DiagnosticCode::UnknownMarker),
        "expected no UnknownMarker diagnostics for \\{marker} in:\n{snippet}"
    );
}

fn snippet_for_marker(marker: &str, kind: MarkerKind) -> String {
    match kind {
        MarkerKind::Header => {
            format!("\\id GEN\n\\{marker} sample\n\\c 1\n\\p \\v 1 text\n")
        }
        MarkerKind::Paragraph => {
            format!("\\id GEN\n\\c 1\n\\{marker} sample text\n")
        }
        MarkerKind::Meta => {
            format!("\\id GEN\n\\{marker} sample\n\\c 1\n\\p \\v 1 text\n")
        }
        MarkerKind::Chapter => "\\id GEN\n\\c 1\n\\p \\v 1 text\n".to_string(),
        MarkerKind::Verse => "\\id GEN\n\\c 1\n\\p \\v 1 text\n".to_string(),
        MarkerKind::Character => {
            format!("\\id GEN\n\\c 1\n\\p \\v 1 \\{marker} text\\{marker}*\n")
        }
        MarkerKind::Note => {
            format!("\\id GEN\n\\c 1\n\\p \\v 1 \\{marker} + \\ft note\\ft*\\{marker}*\n")
        }
        MarkerKind::MilestoneStart => {
            if let Some(base) = marker.strip_suffix("-s") {
                format!(
                    "\\id GEN\n\\c 1\n\\p \\v 1 \\{marker} |who=\"speaker\"\\* text \\{base}-e\\*\n"
                )
            } else {
                format!("\\id GEN\n\\c 1\n\\p \\v 1 \\{marker}\\*\n")
            }
        }
        MarkerKind::MilestoneEnd => {
            if let Some(base) = marker.strip_suffix("-e") {
                format!(
                    "\\id GEN\n\\c 1\n\\p \\v 1 \\{base}-s |who=\"speaker\"\\* text \\{marker}\\*\n"
                )
            } else {
                format!("\\id GEN\n\\c 1\n\\p \\v 1 \\{marker}\\*\n")
            }
        }
        MarkerKind::SidebarStart => {
            "\\id GEN\n\\c 1\n\\esb\n\\p sidebar text\n\\esbe\n".to_string()
        }
        MarkerKind::SidebarEnd => "\\id GEN\n\\c 1\n\\esb\n\\p sidebar text\n\\esbe\n".to_string(),
        MarkerKind::Figure => "\\id GEN\n\\c 1\n\\p \\v 1 \\fig Caption\\fig*\n".to_string(),
        MarkerKind::Periph => "\\periph foreword\n\\p peripheral text\n".to_string(),
        MarkerKind::TableRow => "\\id GEN\n\\c 1\n\\tr \\tc1 row cell\n".to_string(),
        MarkerKind::TableCell => format!("\\id GEN\n\\c 1\n\\tr \\{marker} cell\n"),
        MarkerKind::Unknown => {
            unreachable!("documented-marker tests should never generate Unknown")
        }
    }
}

#[test]
fn docs_usfm_bible_31_exact_markers_are_known_and_classified() {
    for marker in DOCS_USFM_BIBLE_31.exact {
        assert_registry_match(marker.marker, marker.kind, DOCS_USFM_BIBLE_31.name);
        assert_exact_marker_is_known(marker.marker, DOCS_USFM_BIBLE_31.name);
    }
}

#[test]
fn docs_usfm_bible_31_numbered_families_are_classified() {
    for pattern in DOCS_USFM_BIBLE_31.patterns {
        for marker in pattern.samples {
            assert_registry_match(marker, lookup_marker(marker).kind, DOCS_USFM_BIBLE_31.name);
            assert_eq!(
                lookup_marker(marker).kind,
                pattern.kind,
                "{}: expected {} sample \\{} to classify as {:?}",
                DOCS_USFM_BIBLE_31.name,
                pattern.notation,
                marker,
                pattern.kind
            );
        }
    }
}

#[test]
fn ubsicap_30_exact_markers_are_known_and_classified() {
    for marker in UBSICAP_30.exact {
        assert_registry_match(marker.marker, marker.kind, UBSICAP_30.name);
        assert_exact_marker_is_known(marker.marker, UBSICAP_30.name);
    }
}

#[test]
fn ubsicap_30_numbered_families_are_classified() {
    for pattern in UBSICAP_30.patterns {
        for marker in pattern.samples {
            assert_registry_match(marker, lookup_marker(marker).kind, UBSICAP_30.name);
            assert_eq!(
                lookup_marker(marker).kind,
                pattern.kind,
                "{}: expected {} sample \\{} to classify as {:?}",
                UBSICAP_30.name,
                pattern.notation,
                marker,
                pattern.kind
            );
        }
    }
}

#[test]
fn documented_marker_samples_parse_without_unknown_diagnostics() {
    for marker in DOCS_USFM_BIBLE_31.exact {
        parse_has_no_unknown_marker(marker.marker, marker.kind);
    }
    for pattern in DOCS_USFM_BIBLE_31.patterns {
        for marker in pattern.samples {
            parse_has_no_unknown_marker(marker, pattern.kind);
        }
    }

    for marker in UBSICAP_30.exact {
        parse_has_no_unknown_marker(marker.marker, marker.kind);
    }
    for pattern in UBSICAP_30.patterns {
        for marker in pattern.samples {
            parse_has_no_unknown_marker(marker, pattern.kind);
        }
    }
}

#[test]
fn source_specific_documentation_differences_are_tracked() {
    let docs_only: BTreeSet<_> = source_families(DOCS_USFM_BIBLE_31)
        .difference(&source_families(UBSICAP_30))
        .copied()
        .collect();
    let ubs_only: BTreeSet<_> = source_families(UBSICAP_30)
        .difference(&source_families(DOCS_USFM_BIBLE_31))
        .copied()
        .collect();

    assert_eq!(docs_only, BTreeSet::from(["efe", "tcc", "thc"]));
    assert_eq!(ubs_only, BTreeSet::from(["ndx"]));
}
