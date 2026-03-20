#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataTarget {
    Chapter,
    Verse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataWindow {
    Chapter,
    Verse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataMarker {
    Ca,
    Cp,
    Va,
    Vp,
}

impl MetadataMarker {
    pub(crate) fn from_marker(marker: &str) -> Option<Self> {
        match marker {
            "ca" => Some(Self::Ca),
            "cp" => Some(Self::Cp),
            "va" => Some(Self::Va),
            "vp" => Some(Self::Vp),
            _ => None,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Ca => "ca",
            Self::Cp => "cp",
            Self::Va => "va",
            Self::Vp => "vp",
        }
    }

    pub(crate) fn target(self) -> MetadataTarget {
        match self {
            Self::Ca | Self::Cp => MetadataTarget::Chapter,
            Self::Va | Self::Vp => MetadataTarget::Verse,
        }
    }

    pub(crate) fn binds_in(self, window: MetadataWindow) -> bool {
        matches!(
            (self, window),
            (Self::Ca | Self::Cp, MetadataWindow::Chapter)
                | (Self::Va | Self::Vp, MetadataWindow::Verse)
        )
    }

    pub(crate) fn allows_literal_inline(self) -> bool {
        matches!(self, Self::Va)
    }
}
