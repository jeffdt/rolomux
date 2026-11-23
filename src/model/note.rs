//! Condensing and wrapping of the free-text note an agent reports for a pane.

fn strip_fences(note: &str) -> String {
    let mut out = String::new();
    for (i, part) in note.split("```").enumerate() {
        if i % 2 == 0 {
            out.push_str(part);
            out.push(' ');
        }
    }
    out
}

fn sentences(text: &str) -> Vec<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = collapsed.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;
    for i in 0..chars.len() {
        let ends = matches!(chars[i], '.' | '?' | '!')
            && chars.get(i + 1).is_none_or(|c| c.is_whitespace());
        if ends || i + 1 == chars.len() {
            let s: String = chars[start..=i].iter().collect();
            out.push(s);
            start = i + 1;
        }
    }
    out.into_iter()
        .map(|s| {
            s.trim()
                .trim_start_matches(['#', '-', '*', '>', ' '])
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// Reduces a note to one sentence: the last question if any, else the first sentence.
#[allow(dead_code)] // the agent view uses this next
pub fn condense(note: &str) -> String {
    let all = sentences(&strip_fences(note));
    all.iter()
        .rev()
        .find(|s| s.ends_with('?'))
        .or_else(|| all.first())
        .cloned()
        .unwrap_or_default()
}

/// Wraps `text` on word boundaries into at most `lines` lines of at most `width` chars,
/// ending the last line with `…` when text remains.
#[allow(dead_code)] // the agent view uses this next
pub fn wrap_note(text: &str, width: usize, lines: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if width == 0 || lines == 0 {
        return out;
    }
    let mut words: std::collections::VecDeque<String> =
        text.split_whitespace().map(str::to_string).collect();
    let mut cur = String::new();
    while let Some(word) = words.pop_front() {
        let len = word.chars().count();
        let cur_len = cur.chars().count();
        if cur.is_empty() && len > width {
            let head: String = word.chars().take(width).collect();
            let tail: String = word.chars().skip(width).collect();
            words.push_front(tail);
            cur = head;
        } else if cur.is_empty() {
            cur = word;
            continue;
        } else if cur_len + 1 + len <= width {
            cur.push(' ');
            cur.push_str(&word);
            continue;
        } else {
            words.push_front(word);
        }
        out.push(std::mem::take(&mut cur));
        if out.len() == lines {
            break;
        }
    }
    if out.len() < lines && !cur.is_empty() {
        out.push(cur);
    } else if out.len() == lines && (!words.is_empty() || !cur.is_empty()) {
        ellipsize_last(&mut out, width);
    }
    out
}

fn ellipsize_last(out: &mut [String], width: usize) {
    let last = out.last_mut().expect("non-empty");
    let mut chars: Vec<char> = last.chars().collect();
    chars.truncate(width.saturating_sub(1));
    if let Some(pos) = chars.iter().rposition(|c| *c == ' ') {
        chars.truncate(pos);
    }
    let mut s: String = chars.into_iter().collect();
    s.push('…');
    *last = s;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn question_beats_headline() {
        assert_eq!(condense("Done with the refactor. Should I merge it?"), "Should I merge it?");
    }

    #[test]
    fn last_of_several_questions() {
        assert_eq!(condense("A? B. C? D."), "C?");
    }

    #[test]
    fn headline_when_no_question() {
        assert_eq!(condense("All green.  Pushed to main."), "All green.");
    }

    #[test]
    fn fenced_code_ignored() {
        assert_eq!(condense("Fixed it.\n```\nwhy? because.\n```\nDone."), "Fixed it.");
        assert_eq!(condense("Hello.\n```\nunterminated? yes"), "Hello.");
    }

    #[test]
    fn markdown_markers_stripped() {
        assert_eq!(condense("## Summary\n- first point"), "Summary - first point");
        assert_eq!(condense("- did the thing. more"), "did the thing.");
        assert_eq!(condense("> * quoted."), "quoted.");
    }

    #[test]
    fn empty_input() {
        assert_eq!(condense(""), "");
        assert_eq!(condense("  \n\t "), "");
    }

    #[test]
    fn wrap_fits() {
        assert_eq!(wrap_note("hello world", 20, 2), vec!["hello world"]);
    }

    #[test]
    fn wrap_on_words() {
        assert_eq!(wrap_note("aaa bbb ccc", 7, 3), vec!["aaa bbb", "ccc"]);
    }

    #[test]
    fn wrap_ellipsizes_overflow() {
        assert_eq!(wrap_note("aaa bbb ccc ddd eee", 7, 2), vec!["aaa bbb", "ccc…"]);
    }

    #[test]
    fn wrap_hard_cuts_long_word() {
        assert_eq!(wrap_note("abcdefghij", 4, 3), vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn wrap_counts_chars() {
        assert_eq!(wrap_note("héllo wörld", 5, 2), vec!["héllo", "wörld"]);
        let out = wrap_note("ééééé ééééé ééééé", 5, 2);
        assert_eq!(out.len(), 2);
        assert!(out[1].chars().count() <= 5 && out[1].ends_with('…'));
    }
}
