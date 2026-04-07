use serde::Serialize;

use crate::diagnostics::Span;

/// Source-location metadata for a structural node.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SourceSpans {
    pub node: Span,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<Span>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<Span>,
}

impl SourceSpans {
    pub fn node(span: Span) -> Self {
        Self {
            node: span,
            code: None,
            number: None,
            close: None,
        }
    }

    pub fn with_code(mut self, span: Span) -> Self {
        self.code = Some(span);
        self
    }

    pub fn with_number(mut self, span: Span) -> Self {
        self.number = Some(span);
        self
    }

    pub fn with_close(mut self, span: Span) -> Self {
        self.close = Some(span);
        self
    }
}

/// Source-map tree for a semantic AST document.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SourceMap {
    pub content: Vec<SourceNode>,
}

/// Source-map entry aligned one-for-one with an AST node.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct SourceNode {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spans: Option<SourceSpans>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SourceNode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor_cst: Option<usize>,
}

impl SourceNode {
    pub fn structural(spans: SourceSpans, children: Vec<SourceNode>, anchor_cst: Option<usize>) -> Self {
        Self {
            spans: Some(spans),
            children,
            anchor_cst,
        }
    }

    pub fn leaf() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_spans_builders_attach_optional_metadata() {
        let spans = SourceSpans::node(10..20)
            .with_code(11..14)
            .with_number(15..16)
            .with_close(18..20);

        assert_eq!(spans.node, 10..20);
        assert_eq!(spans.code, Some(11..14));
        assert_eq!(spans.number, Some(15..16));
        assert_eq!(spans.close, Some(18..20));
    }

    #[test]
    fn structural_source_node_keeps_spans_children_and_anchor() {
        let node = SourceNode::structural(
            SourceSpans::node(0..20).with_code(0..3),
            vec![SourceNode::leaf()],
            Some(7),
        );

        assert_eq!(node.spans.as_ref().unwrap().code, Some(0..3));
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.anchor_cst, Some(7));
    }

    #[test]
    fn leaf_source_node_is_empty() {
        let node = SourceNode::leaf();

        assert!(node.spans.is_none());
        assert!(node.children.is_empty());
        assert!(node.anchor_cst.is_none());
    }
}
