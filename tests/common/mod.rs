//! Checks shared by the test files.

#![allow(dead_code)]

const WIDTH: usize = 80;

/// The compiler's syntax tree for `src`, without spans or ids, which move
/// when the layout changes.
fn tree(src: &str) -> String {
    let program = rill::lang::parse(src)
        .unwrap_or_else(|e| panic!("the compiler rejects:\n{src}\n{e:?}"));
    let debug = format!("{:?}", program.items);
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug.as_str();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("Span { start: ") {
            rest = &after[after.find('}').unwrap() + 1..];
            out.push('S');
        } else if let Some(after) = rest.strip_prefix("id: ") {
            rest = after.trim_start_matches(|c: char| c.is_ascii_digit());
            out.push_str("id");
        } else {
            let c = rest.chars().next().unwrap();
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// Every word of every comment, in order, without the comment markers.
fn comment_words(src: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find('/') {
        let after = &rest[i..];
        let end = if after.starts_with("//") {
            after.find('\n').unwrap_or(after.len())
        } else if after.starts_with("/*") {
            after.find("*/").map_or(after.len(), |e| e + 2)
        } else {
            rest = &after[1..];
            continue;
        };
        // Wrapping repeats the `//` marker, so markers are not words.
        words.extend(
            after[..end]
                .split_whitespace()
                .map(|w| w.trim_start_matches(['/', '!']).trim_end_matches("*/"))
                .filter(|w| !w.is_empty())
                .map(str::to_owned),
        );
        rest = &after[end..];
    }
    words
}

pub fn lf(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Everything that must hold for any input the compiler accepts.
pub fn check_properties(name: &str, src: &str) -> String {
    let out = flume::format(src).unwrap_or_else(|e| panic!("{name}: {e}"));

    assert_eq!(tree(src), tree(&out), "{name}: formatting changed the meaning");

    let again = flume::format(&out).unwrap();
    assert_eq!(lf(&out), lf(&again), "{name}: formatting twice changed it");

    assert!(!out.contains('\t'), "{name}: output has a tab");
    let bare_lf = out.replace("\r\n", "").contains('\n');
    let bare_cr = out.replace("\r\n", "").contains('\r');
    assert!(!bare_lf && !bare_cr, "{name}: line ending other than CRLF");
    assert!(out.is_empty() || out.ends_with("\r\n"), "{name}: no final newline");
    assert!(!out.ends_with("\r\n\r\n"), "{name}: blank line at the end");

    for line in lf(&out).lines() {
        assert_eq!(line, line.trim_end(), "{name}: trailing whitespace");
        let width = line.chars().count();
        // A single word longer than the line cannot be broken.
        let breakable = line.trim().contains(' ');
        assert!(
            width <= WIDTH || !breakable || line.contains("/*"),
            "{name}: {width} columns:\n{line}"
        );
    }

    // Comments too long for their line move above it, past any other
    // comment on that line, so only the set of words must match.
    let mut before = comment_words(src);
    let mut after = comment_words(&out);
    before.sort();
    after.sort();
    assert_eq!(before, after, "{name}: comments changed");
    out
}

