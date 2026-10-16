//! Minimal change lists between two snapshots of a keyed list, so Qt
//! models can update rows in place instead of resetting.

/// Apply in the order produced: updates (indices valid before structural
/// changes), then the remove, then the insert.
#[derive(Debug, Clone)]
pub enum ListChange<T> {
    Update { at: usize, row: T },
    Remove { at: usize, count: usize },
    Insert { at: usize, rows: Vec<T> },
}

/// Matches a common prefix and suffix by key and replaces the middle.
/// Changes in these lists are local (a streaming message, one session's
/// status), so this stays small without a general LCS diff.
pub fn diff_by<T: Clone>(
    old: &[T],
    new: &[T],
    same_key: impl Fn(&T, &T) -> bool,
    same_value: impl Fn(&T, &T) -> bool,
) -> Vec<ListChange<T>> {
    let mut changes = Vec::new();
    let mut prefix = 0;
    while prefix < old.len() && prefix < new.len() && same_key(&old[prefix], &new[prefix]) {
        if !same_value(&old[prefix], &new[prefix]) {
            changes.push(ListChange::Update { at: prefix, row: new[prefix].clone() });
        }
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old.len() - prefix
        && suffix < new.len() - prefix
        && same_key(&old[old.len() - 1 - suffix], &new[new.len() - 1 - suffix])
    {
        let (o, n) = (old.len() - 1 - suffix, new.len() - 1 - suffix);
        if !same_value(&old[o], &new[n]) {
            changes.push(ListChange::Update { at: o, row: new[n].clone() });
        }
        suffix += 1;
    }
    let old_mid = old.len() - prefix - suffix;
    let new_mid = new.len() - prefix - suffix;
    if old_mid > 0 {
        changes.push(ListChange::Remove { at: prefix, count: old_mid });
    }
    if new_mid > 0 {
        changes.push(ListChange::Insert { at: prefix, rows: new[prefix..prefix + new_mid].to_vec() });
    }
    changes
}

/// Applies changes to a plain vector (tests and Rust-side mirrors).
pub fn apply<T>(list: &mut Vec<T>, changes: Vec<ListChange<T>>) {
    for c in changes {
        match c {
            ListChange::Update { at, row } => list[at] = row,
            ListChange::Remove { at, count } => {
                list.drain(at..at + count);
            }
            ListChange::Insert { at, rows } => {
                list.splice(at..at, rows);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(old: &[(char, u8)], new: &[(char, u8)]) -> usize {
        let changes = diff_by(old, new, |a, b| a.0 == b.0, |a, b| a == b);
        let n = changes.len();
        let mut list = old.to_vec();
        apply(&mut list, changes);
        assert_eq!(list, new);
        n
    }

    #[test]
    fn round_trips() {
        assert_eq!(run(&[('a', 0), ('b', 0)], &[('a', 0), ('b', 1)]), 1);
        assert_eq!(run(&[('a', 0)], &[('a', 0), ('b', 0), ('c', 0)]), 1);
        run(&[('a', 0), ('b', 0), ('c', 0), ('z', 0)], &[('a', 0), ('x', 0), ('z', 1)]);
        run(&[('a', 0), ('b', 0)], &[]);
        run(&[], &[('a', 0)]);
        run(&[('a', 0), ('b', 0)], &[('b', 0), ('a', 0)]);
    }
}
