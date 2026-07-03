use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Rank `candidates` against `query` by fuzzy score, best match first.
/// Returns indices into `candidates`; non-matching candidates are omitted.
/// Ties on score are broken by the candidate string ascending, mirroring the
/// model's name-ascending tie-break. `query` is assumed non-empty.
///
/// This is the only place the fuzzy matcher is touched, so the matcher can be
/// swapped without disturbing the model or UI (cf. the `INITIAL_FOCUS` seam).
pub fn rank(query: &str, candidates: &[impl AsRef<str>]) -> Vec<usize> {
    let atom = Atom::new(
        query,
        CaseMatching::Smart,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut scored: Vec<(usize, u16)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let mut buf = Vec::new();
            let hay = Utf32Str::new(c.as_ref(), &mut buf);
            atom.score(hay, &mut matcher).map(|s| (i, s))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| candidates[a.0].as_ref().cmp(candidates[b.0].as_ref()))
    });
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_strong_match_above_weak_earlier_one() {
        // "api" is a scattered subsequence of the first candidate (a..p..i)
        // but a consecutive prefix of the second; the prefix match must win
        // even though it appears later in the list.
        let candidates = ["a-pretty-big-input", "api-gateway"];
        let ranked = rank("api", &candidates);
        assert_eq!(ranked.first().copied(), Some(1), "consecutive prefix ranks first");
    }

    #[test]
    fn omits_non_matches() {
        let candidates = ["alpha", "beta", "gamma"];
        let ranked = rank("zzz", &candidates);
        assert!(ranked.is_empty(), "no candidate contains the subsequence zzz");
    }

    #[test]
    fn ties_break_by_candidate_string_ascending() {
        // Two identical haystacks (same score) -> ascending string order,
        // i.e. the lexicographically smaller original string wins.
        let candidates = ["zebra", "apple"];
        let ranked = rank("a", &candidates);
        // Both contain 'a'; apple should score >= zebra and at worst tie,
        // and on a tie apple (index 1) precedes zebra (index 0).
        assert_eq!(ranked.first().copied(), Some(1), "apple ranks first");
    }

    #[test]
    fn accepts_string_slices_and_owned_strings() {
        let owned = vec![String::from("api-gateway"), String::from("web")];
        let ranked = rank("api", &owned);
        assert_eq!(ranked.first().copied(), Some(0));
    }
}
