//! What formatting would change in a file, for `flume check`.

/// One place where the file differs from its formatted form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    /// Lines from `line` (1-based, in the input) should be replaced.
    Lines {
        line: usize,
        found: Vec<String>,
        expected: Vec<String>,
    },
    /// Some lines do not end in CRLF.
    LineEndings { first: usize },
}

impl std::fmt::Display for Issue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Issue::Lines {
                line,
                found,
                expected,
            } => {
                write!(f, "{line}: not formatted")?;
                for l in found {
                    write!(f, "\n  -{}", visible(l))?;
                }
                for l in expected {
                    write!(f, "\n  +{}", visible(l))?;
                }
                Ok(())
            }
            Issue::LineEndings { first } => {
                write!(f, "{first}: lines must end in CRLF")
            }
        }
    }
}

/// A line for display after a `-` or `+`, with trailing whitespace shown as
/// `·` so a change to it can be seen.
fn visible(line: &str) -> String {
    if line.is_empty() {
        return String::new();
    }
    let code = line.trim_end();
    let tail = line[code.len()..].chars().count();
    format!(" {code}{}", "·".repeat(tail))
}

/// The differences between `src` and `formatted`, which is `src` after
/// formatting.
pub fn issues(src: &str, formatted: &str) -> Vec<Issue> {
    let mut issues = Vec::new();
    let before = lines(src);
    let after = lines(formatted);

    let line_count = src.split('\n').count();
    if let Some(first) = src
        .split('\n')
        .enumerate()
        // The piece after the last `\n` is not a line ending.
        .take(line_count - 1)
        .position(|(_, l)| !l.ends_with('\r'))
    {
        issues.push(Issue::LineEndings { first: first + 1 });
    }

    for (start, end_before, end_after) in hunks(&before, &after) {
        issues.push(Issue::Lines {
            line: start.0 + 1,
            found: before[start.0..end_before].to_vec(),
            expected: after[start.1..end_after].to_vec(),
        });
    }
    if !src.is_empty() && !src.ends_with('\n') && !formatted.is_empty() && issues.is_empty() {
        issues.push(Issue::LineEndings {
            first: before.len(),
        });
    }
    issues
}

/// The text of each line, without its line ending.
fn lines(s: &str) -> Vec<String> {
    let mut lines: Vec<String> = s
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_owned())
        .collect();
    if lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }
    lines
}

/// Regions where `a` and `b` differ: (start in a, start in b), end in a,
/// end in b.
fn hunks(a: &[String], b: &[String]) -> Vec<((usize, usize), usize, usize)> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (a_mid, b_mid) = (&a[prefix..a.len() - suffix], &b[prefix..b.len() - suffix]);
    if a_mid.is_empty() && b_mid.is_empty() {
        return Vec::new();
    }
    // A longest common subsequence splits the middle into small hunks. On a
    // huge middle that costs too much memory, so report it whole.
    if a_mid.len() * b_mid.len() > 4_000_000 {
        return vec![((prefix, prefix), a.len() - suffix, b.len() - suffix)];
    }
    let (n, m) = (a_mid.len(), b_mid.len());
    let mut lcs = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if a_mid[i] == b_mid[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let mut hunks = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut open: Option<(usize, usize)> = None;
    while i < n || j < m {
        if i < n && j < m && a_mid[i] == b_mid[j] {
            if let Some(start) = open.take() {
                hunks.push((start, prefix + i, prefix + j));
            }
            i += 1;
            j += 1;
            continue;
        }
        open.get_or_insert((prefix + i, prefix + j));
        if j < m && (i == n || lcs[i][j + 1] >= lcs[i + 1][j]) {
            j += 1;
        } else {
            i += 1;
        }
    }
    if let Some(start) = open {
        hunks.push((start, prefix + n, prefix + m));
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatted_files_have_no_issues() {
        let src = "rill m() -> S {\r\n    return 0\r\n}\r\n";
        assert_eq!(issues(src, src), []);
    }

    #[test]
    fn reports_each_changed_region() {
        let src = "a\r\nb  \r\nc\r\nd\r\ne x\r\n";
        let formatted = "a\r\nb\r\nc\r\nd\r\ne\r\nx\r\n";
        assert_eq!(
            issues(src, formatted),
            [
                Issue::Lines {
                    line: 2,
                    found: vec!["b  ".into()],
                    expected: vec!["b".into()],
                },
                Issue::Lines {
                    line: 5,
                    found: vec!["e x".into()],
                    expected: vec!["e".into(), "x".into()],
                },
            ]
        );
    }

    #[test]
    fn shows_whitespace_changes() {
        let issue = &issues("a  \r\n", "a\r\n\r\n")[0];
        assert_eq!(issue.to_string(), "1: not formatted\n  - a··\n  + a\n  +");
    }

    #[test]
    fn reports_lf_once() {
        assert_eq!(
            issues("a\nb\n", "a\r\nb\r\n"),
            [Issue::LineEndings { first: 1 }]
        );
        assert_eq!(
            issues("a\r\nb", "a\r\nb\r\n"),
            [Issue::LineEndings { first: 2 }]
        );
    }
}
