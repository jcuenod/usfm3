use logos::Logos;

/// Byte-offset range into the source string.
pub type Span = std::ops::Range<usize>;

/// USFM 3.x token produced by the logos-based lexer.
///
/// Logos matches the longest candidate; declaration order breaks ties for
/// same-length matches.  The ordering below therefore gives priority to the
/// fixed keywords (`\c`, `\v`) over the generic `Marker` catch-all, and to
/// the more-specific nested / closing / milestone patterns over the plain
/// `Marker` pattern.
#[derive(Logos, Debug, Clone, PartialEq)]
pub enum Token<'a> {
    // ── Whitespace ────────────────────────────────────────────────────
    /// Runs of spaces, tabs, and carriage returns.  The builder will
    /// decide contextually whether to emit, normalize, or discard
    /// whitespace.
    #[regex(r"[ \t]+")]
    Whitespace(&'a str),

    // ── Fixed keywords ──────────────────────────────────────────────────
    /// `\c` -- chapter marker (takes a numeric argument as the next token).
    #[token("\\c", priority = 5)]
    Chapter,

    /// `\v` -- verse marker (takes a numeric or range argument as the next token).
    #[token("\\v", priority = 5)]
    Verse,

    // ── Milestones ──────────────────────────────────────────────────────
    /// Milestone markers such as `\qt1-s`, `\qt-e`, `\ts-s`, etc.
    #[regex(r"\\[a-z]+[0-9]*-[se]")]
    Milestone(&'a str),

    // ── Nested markers (USFM 2.4+) ─────────────────────────────────────
    /// `\+marker` -- nested character opening marker.
    #[regex(r"\\\+[a-z]+[0-9]*")]
    NestedMarker(&'a str),

    /// `\+marker*` -- nested character closing marker.
    #[regex(r"\\\+[a-z]+[0-9]*\*")]
    NestedClosingMarker(&'a str),

    // ── Milestone terminator ────────────────────────────────────────────
    /// `\*` -- milestone attribute block terminator.
    #[token("\\*", priority = 5)]
    MilestoneEnd,

    // ── Regular markers ─────────────────────────────────────────────────
    /// `\marker*` -- character / note closing marker.
    #[regex(r"\\[a-z]+[0-9]*(-[0-9]+)?\*")]
    ClosingMarker(&'a str),

    /// `\marker` -- paragraph or character opening marker (catch-all).
    /// The optional `-[0-9]+` suffix handles column-spanning table markers
    /// like `\tcr1-2` (spanning columns 1-2).
    #[regex(r"\\[a-z]+[0-9]*(-[0-9]+)?", priority = 2)]
    Marker(&'a str),

    // ── Attributes ──────────────────────────────────────────────────────
    /// Attribute block starting with `|`, e.g. `|lemma="grace" strong="H1234"`.
    /// The regex allows `\"` (escaped quotes) within the attribute string.
    #[regex(r#"\|(?:[^\\\n]|\\")+"#)]
    Attributes(&'a str),

    // ── Text ────────────────────────────────────────────────────────────
    /// Any run of text that is not a backslash, newline, or pipe.
    /// The first character must not be a space, tab, or CR (to avoid conflict
    /// with the skip pattern), but subsequent characters may include whitespace.
    #[regex(r"[^ \t\r\\\n|][^\\\r\n|]*")]
    Text(&'a str),

    // ── Structural ──────────────────────────────────────────────────────
    /// A newline character -- significant because paragraph boundaries in
    /// USFM often coincide with newlines.
    #[regex(r"\r?\n")]
    Newline,
}

/// Tokenize a USFM source string.
///
/// Logos errors (unrecognised byte sequences) are folded into [`Token::Text`]
/// so that the function **never** fails -- every byte of the input is accounted
/// for in the returned vector.
pub fn tokenize(input: &str) -> Vec<(Token<'_>, Span)> {
    let lexer = Token::lexer(input);
    let mut tokens: Vec<(Token<'_>, Span)> = Vec::new();

    for (result, span) in lexer.spanned() {
        match result {
            Ok(token) => tokens.push((token, span)),
            Err(()) => {
                // Unrecognised bytes -- wrap them in a Text token so that no
                // information is lost and the tokenizer never panics.
                let slice = &input[span.clone()];
                tokens.push((Token::Text(slice), span));
            }
        }
    }

    tokens
}

/// Strip the leading backslash from a marker string.
///
/// ```text
/// \p   -> p
/// \+nd -> +nd
/// \c   -> c
/// ```
///
/// If the input does not start with `\`, the string is returned unchanged.
pub fn strip_marker_backslash(marker: &str) -> &str {
    marker.strip_prefix('\\').unwrap_or(marker)
}

/// Strip the leading backslash **and** trailing `*` from a closing marker string.
///
/// ```text
/// \nd*  -> nd
/// \+nd* -> +nd
/// ```
///
/// The function strips a leading `\` and a trailing `*` independently;
/// if either is absent the rest of the transformation still applies.
pub fn strip_closing_star(marker: &str) -> &str {
    let s = marker.strip_prefix('\\').unwrap_or(marker);
    s.strip_suffix('*').unwrap_or(s)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Individual token recognition ────────────────────────────────────

    #[test]
    fn test_marker() {
        let tokens = tokenize(r"\p");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Marker(r"\p"));
    }

    #[test]
    fn test_chapter() {
        let tokens = tokenize(r"\c");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Chapter);
    }

    #[test]
    fn test_verse() {
        let tokens = tokenize(r"\v");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Verse);
    }

    #[test]
    fn test_milestone() {
        let tokens = tokenize(r"\qt1-s");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Milestone(r"\qt1-s"));

        let tokens = tokenize(r"\qt-e");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Milestone(r"\qt-e"));

        let tokens = tokenize(r"\ts-s");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Milestone(r"\ts-s"));
    }

    #[test]
    fn test_nested_marker() {
        let tokens = tokenize(r"\+nd");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::NestedMarker(r"\+nd"));
    }

    #[test]
    fn test_nested_closing_marker() {
        let tokens = tokenize(r"\+nd*");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::NestedClosingMarker(r"\+nd*"));
    }

    #[test]
    fn test_closing_marker() {
        let tokens = tokenize(r"\nd*");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::ClosingMarker(r"\nd*"));
    }

    #[test]
    fn test_text_content() {
        let tokens = tokenize("Hello world");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Text("Hello world"));
    }

    #[test]
    fn test_attributes() {
        let input = r#"|lemma="grace" strong="H1234""#;
        let tokens = tokenize(input);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Attributes(input));
    }

    #[test]
    fn test_newline() {
        let tokens = tokenize("\n");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Newline);
    }

    // ── Spans ───────────────────────────────────────────────────────────

    #[test]
    fn test_spans_are_correct() {
        let input = r"\p Hello";
        let tokens = tokenize(input);
        assert_eq!(tokens.len(), 3);
        // \p occupies bytes 0..2
        assert_eq!(tokens[0].1, 0..2);
        // Whitespace at byte 2..3
        assert_eq!(tokens[1].0, Token::Whitespace(" "));
        assert_eq!(tokens[1].1, 2..3);
        // "Hello" starts after "\p " (3 bytes)
        assert_eq!(tokens[2].1, 3..8);
        assert_eq!(&input[tokens[2].1.clone()], "Hello");
    }

    // ── Complete USFM snippet ───────────────────────────────────────────

    #[test]
    fn test_complete_usfm_snippet() {
        let input = "\\id GEN\n\\c 1\n\\p\n\\v 1 In the beginning";
        let tokens = tokenize(input);

        // Expected sequence (with explicit Whitespace tokens):
        // \id  -> Marker
        //      -> Whitespace
        // GEN  -> Text
        // \n   -> Newline
        // \c   -> Chapter
        //      -> Whitespace
        // 1    -> Text
        // \n   -> Newline
        // \p   -> Marker
        // \n   -> Newline
        // \v   -> Verse
        //      -> Whitespace
        // 1 In the beginning -> Text (spaces within text are preserved)
        let kinds: Vec<&str> = tokens
            .iter()
            .map(|(tok, _)| match tok {
                Token::Chapter => "Chapter",
                Token::Verse => "Verse",
                Token::Milestone(_) => "Milestone",
                Token::NestedMarker(_) => "NestedMarker",
                Token::NestedClosingMarker(_) => "NestedClosingMarker",
                Token::ClosingMarker(_) => "ClosingMarker",
                Token::MilestoneEnd => "MilestoneEnd",
                Token::Marker(_) => "Marker",
                Token::Attributes(_) => "Attributes",
                Token::Text(_) => "Text",
                Token::Whitespace(_) => "Whitespace",
                Token::Newline => "Newline",
            })
            .collect();

        assert_eq!(
            kinds,
            vec![
                "Marker",     // \id
                "Whitespace", // " "
                "Text",       // GEN
                "Newline",    // \n
                "Chapter",    // \c
                "Whitespace", // " "
                "Text",       // 1
                "Newline",    // \n
                "Marker",     // \p
                "Newline",    // \n
                "Verse",      // \v
                "Whitespace", // " "
                "Text",       // 1 In the beginning
            ]
        );

        // Verify specific token contents
        assert_eq!(tokens[0].0, Token::Marker("\\id"));
        assert_eq!(tokens[2].0, Token::Text("GEN"));
        assert_eq!(tokens[4].0, Token::Chapter);
        assert_eq!(tokens[6].0, Token::Text("1"));
        assert_eq!(tokens[8].0, Token::Marker("\\p"));
        assert_eq!(tokens[10].0, Token::Verse);
        assert_eq!(tokens[12].0, Token::Text("1 In the beginning"));
    }

    // ── Error recovery ──────────────────────────────────────────────────

    #[test]
    fn test_invalid_bytes_no_panic() {
        // A bare backslash followed by non-alpha chars should not panic.
        let input = "\\ \\\n\\123";
        let _tokens = tokenize(input);
        // We only care that it doesn't panic; the exact token count depends
        // on how logos partitions the unrecognised bytes.
    }

    #[test]
    fn test_error_recovery_produces_text() {
        // A lone backslash isn't valid USFM -- the lexer should wrap it in Text.
        let input = "\\";
        let tokens = tokenize(input);
        assert!(!tokens.is_empty(), "should produce at least one token");
        // The error path wraps the slice as Text.
        match &tokens[0].0 {
            Token::Text(s) => assert_eq!(*s, "\\"),
            other => panic!("expected Text for bare backslash, got {:?}", other),
        }
    }

    // ── Helper functions ────────────────────────────────────────────────

    #[test]
    fn test_strip_marker_backslash() {
        assert_eq!(strip_marker_backslash(r"\p"), "p");
        assert_eq!(strip_marker_backslash(r"\+nd"), "+nd");
        assert_eq!(strip_marker_backslash(r"\c"), "c");
        assert_eq!(strip_marker_backslash("p"), "p"); // no backslash -- unchanged
    }

    #[test]
    fn test_strip_closing_star() {
        assert_eq!(strip_closing_star(r"\nd*"), "nd");
        assert_eq!(strip_closing_star(r"\+nd*"), "+nd");
        assert_eq!(strip_closing_star("nd*"), "nd"); // no backslash prefix
        assert_eq!(strip_closing_star(r"\nd"), "nd"); // no star suffix
    }

    // ── Whitespace handling ─────────────────────────────────────────────

    #[test]
    fn test_spaces_and_tabs_are_whitespace_tokens() {
        // Spaces and tabs between tokens are now explicit Whitespace tokens.
        let tokens = tokenize("\\p \t \\v");
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0].0, Token::Marker("\\p"));
        assert_eq!(tokens[1].0, Token::Whitespace(" \t "));
        assert_eq!(tokens[2].0, Token::Verse);
    }

    #[test]
    fn test_marker_with_digits() {
        let tokens = tokenize(r"\q2");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].0, Token::Marker(r"\q2"));
    }

    // ── Whitespace preservation before closing markers ──────────────────
    // Per USFM spec: "Normalized whitespace preceding the closing marker
    // of a character or note marker pair is preserved."

    #[test]
    fn test_trailing_space_before_closing_marker_preserved() {
        let tokens = tokenize(r"\it testimony \it*");
        let text_tokens: Vec<&str> = tokens
            .iter()
            .filter_map(|t| {
                if let Token::Text(s) = &t.0 {
                    Some(*s)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text_tokens, vec!["testimony "]);
    }

    #[test]
    fn test_trailing_space_before_nested_closing_marker_preserved() {
        let tokens = tokenize(r"\+nd Lord \+nd*");
        let text_tokens: Vec<&str> = tokens
            .iter()
            .filter_map(|t| {
                if let Token::Text(s) = &t.0 {
                    Some(*s)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text_tokens, vec!["Lord "]);
    }

    #[test]
    fn test_no_trailing_space_when_no_space_before_close() {
        let tokens = tokenize(r"\it man\it*");
        let text_tokens: Vec<&str> = tokens
            .iter()
            .filter_map(|t| {
                if let Token::Text(s) = &t.0 {
                    Some(*s)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text_tokens, vec!["man"]);
    }
}
