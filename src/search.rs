use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// One searchable session: its name plus the windows that can also match it,
/// each tagged with a caller-chosen id (the model passes window positions).
pub struct Candidate<'a> {
    pub name: &'a str,
    pub windows: Vec<(usize, &'a str)>,
}

/// A matching candidate: its index into the input, whether the name itself
/// matched, and the ids of the windows that matched, in input order.
#[derive(Debug, PartialEq)]
pub struct Hit {
    pub index: usize,
    pub name_matched: bool,
    pub windows: Vec<usize>,
}

/// Rank `candidates` against `query` by fuzzy score, best match first.
/// Non-matching candidates are omitted. The name and each window name are
/// scored separately and the best one counts, so a query's letters never
/// span two names. Ties go to a name match over a window-only match, then to
/// the name ascending (the model's name-ascending tie-break). `query` is
/// assumed non-empty.
///
/// This is the only place the fuzzy matcher is touched, so the matcher can be
/// swapped without disturbing the model or UI (cf. the `INITIAL_FOCUS` seam).
pub fn rank(query: &str, candidates: &[Candidate]) -> Vec<Hit> {
    let atom = Atom::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut score = |s: &str| atom.score(Utf32Str::new(s, &mut buf), &mut matcher);
    let mut scored: Vec<(Hit, u16)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, c)| {
            let name_score = score(c.name);
            let mut best = name_score;
            let mut windows = Vec::new();
            for &(id, w) in &c.windows {
                if let Some(s) = score(w) {
                    windows.push(id);
                    best = best.max(Some(s));
                }
            }
            best.map(|b| (Hit { index, name_matched: name_score.is_some(), windows }, b))
        })
        .collect();
    scored.sort_by(|(a, sa), (b, sb)| {
        sb.cmp(sa)
            .then_with(|| b.name_matched.cmp(&a.name_matched))
            .then_with(|| candidates[a.index].name.cmp(candidates[b.index].name))
    });
    scored.into_iter().map(|(hit, _)| hit).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names<'a>(names: &[&'a str]) -> Vec<Candidate<'a>> {
        names.iter().map(|n| Candidate { name: n, windows: vec![] }).collect()
    }

    fn indices(hits: Vec<Hit>) -> Vec<usize> {
        hits.into_iter().map(|h| h.index).collect()
    }

    #[test]
    fn ranks_strong_match_above_weak_earlier_one() {
        // "api" is a scattered subsequence of the first candidate (a..p..i)
        // but a consecutive prefix of the second; the prefix match must win
        // even though it appears later in the list.
        let ranked = indices(rank("api", &names(&["a-pretty-big-input", "api-gateway"])));
        assert_eq!(ranked.first().copied(), Some(1), "consecutive prefix ranks first");
    }

    #[test]
    fn omits_non_matches() {
        let ranked = rank("zzz", &names(&["alpha", "beta", "gamma"]));
        assert!(ranked.is_empty(), "no candidate contains the subsequence zzz");
    }

    #[test]
    fn ties_break_by_candidate_string_ascending() {
        // Both contain 'a'; apple should score >= zebra and at worst tie,
        // and on a tie apple (index 1) precedes zebra (index 0).
        let ranked = indices(rank("a", &names(&["zebra", "apple"])));
        assert_eq!(ranked.first().copied(), Some(1), "apple ranks first");
    }

    #[test]
    fn query_letters_never_span_the_name_and_a_window() {
        let candidates = vec![Candidate { name: "ab", windows: vec![(0, "cd")] }];
        assert!(rank("bc", &candidates).is_empty());
    }

    #[test]
    fn name_match_beats_an_equal_window_match() {
        let candidates = vec![
            Candidate { name: "other", windows: vec![(0, "logs")] },
            Candidate { name: "logs", windows: vec![] },
        ];
        assert_eq!(indices(rank("logs", &candidates)), vec![1, 0]);
    }
}
