/// A line diff from `old` to `new`: unchanged lines start with two spaces,
/// removed ones with `- ` and added ones with `+ `. Test outputs are short, so
/// the quadratic longest-common-subsequence table is cheap.
pub fn diff(old: &str, new: &str) -> String {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    // common[i][j]: length of the longest common subsequence of old[i..] and new[j..].
    let mut common = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i][j] = if old[i] == new[j] {
                common[i + 1][j + 1] + 1
            } else {
                common[i + 1][j].max(common[i][j + 1])
            };
        }
    }
    let mut out = String::new();
    let mut line = |prefix: &str, text: &str| {
        out.push_str(prefix);
        out.push_str(text);
        out.push('\n');
    };
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            line("  ", old[i]);
            i += 1;
            j += 1;
        } else if j == new.len() || (i < old.len() && common[i + 1][j] >= common[i][j + 1]) {
            line("- ", old[i]);
            i += 1;
        } else {
            line("+ ", new[j]);
            j += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_removed_and_added_lines() {
        assert_eq!(
            diff("a\nb\nc\n", "a\nx\nc\nd\n"),
            "  a\n- b\n+ x\n  c\n+ d\n"
        );
    }

    #[test]
    fn handles_empty_sides() {
        assert_eq!(diff("", "a\n"), "+ a\n");
        assert_eq!(diff("a\n", ""), "- a\n");
        assert_eq!(diff("", ""), "");
    }
}
