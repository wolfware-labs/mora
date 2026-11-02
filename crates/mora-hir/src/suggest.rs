/// The candidate closest to `name`, for a name that does not resolve.
///
/// The distance is the optimal string alignment distance over characters:
/// the fewest insertions, deletions, substitutions and swaps of two
/// adjacent characters that turn one name into the other, with no
/// character edited twice. A candidate is close enough when its distance is
/// at most a third of the length of `name`, and at least 1. The closest one
/// wins, and of several at the same distance the first in name order.
pub(crate) fn closest<'a>(name: &str, candidates: &'a [String]) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .iter()
        .filter(|candidate| candidate.as_str() != name)
        .map(|candidate| (distance(name, candidate), candidate.as_str()))
        .filter(|&(distance, _)| distance <= limit)
        .min()
        .map(|(_, candidate)| candidate)
}

/// The optimal string alignment distance between `a` and `b`.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    // rows[i][j] is the distance between the first i characters of `a` and
    // the first j of `b`.
    let mut rows = vec![vec![0; b.len() + 1]; a.len() + 1];
    for (i, row) in rows.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in rows[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (rows[i - 1][j] + 1)
                .min(rows[i][j - 1] + 1)
                .min(rows[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(rows[i - 2][j - 2] + 1);
            }
            rows[i][j] = best;
        }
    }
    rows[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|&name| name.to_owned()).collect()
    }

    #[test]
    fn distance_counts_edits_and_adjacent_swaps() {
        assert_eq!(distance("", ""), 0);
        assert_eq!(distance("abc", ""), 3);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("User", "User"), 0);
        assert_eq!(distance("Usr", "User"), 1);
        assert_eq!(distance("Users", "User"), 1);
        assert_eq!(distance("Uzer", "User"), 1);
        assert_eq!(distance("Uesr", "User"), 1);
        assert_eq!(distance("ab", "ba"), 1);
        assert_eq!(distance("ca", "abc"), 3);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("Ünïcode", "Unicode"), 2);
    }

    #[test]
    fn the_limit_is_a_third_of_the_length_and_at_least_one() {
        // Two characters allow one edit.
        assert_eq!(closest("Ab", &names(&["Ax"])), Some("Ax"));
        assert_eq!(closest("Ab", &names(&["xy"])), None);
        // Six allow two, five only one.
        assert_eq!(closest("Strinx", &names(&["string"])), Some("string"));
        assert_eq!(closest("Strnx", &names(&["string"])), None);
        assert_eq!(closest("Strng", &names(&["string"])), None);
        assert_eq!(closest("strng", &names(&["string"])), Some("string"));
    }

    #[test]
    fn the_closest_wins_and_ties_go_to_name_order() {
        let candidates = names(&["Users", "User", "Uses"]);
        assert_eq!(closest("Usr", &candidates), Some("User"));
        assert_eq!(closest("Usex", &names(&["Usez", "Usea"])), Some("Usea"));
        assert_eq!(closest("User", &names(&["User"])), None);
        assert_eq!(closest("User", &[]), None);
    }
}
