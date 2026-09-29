//! Diff types: insert, update, remove, reset.
//!
//! A view is the result of a store query. When the data changes, the actor runs the query
//! again and sends the difference between the old and the new list. A frontend that
//! applies the diffs in order to its copy of the list gets the new list.

use std::collections::HashMap;
use std::hash::Hash;

/// A change to a list. Indexes refer to the list after all earlier diffs of the batch.
#[derive(Clone, Debug, PartialEq)]
pub enum ListDiff<T> {
    Insert {
        index: usize,
        item: T,
    },
    Update {
        index: usize,
        item: T,
    },
    Remove {
        index: usize,
    },
    /// Replace the whole list. Always the first diff of a subscription.
    Reset(Vec<T>),
}

/// The JSON shape of a `ListDiff`: `{"type": "insert", "index": 0, "item": {...}}`, and
/// `{"type": "reset", "items": [...]}`. A newtype variant with a list cannot carry an
/// internal tag, so this borrowed copy has the `Reset` fields by name.
#[cfg(feature = "serde")]
#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum DiffRepr<'a, T> {
    Insert { index: usize, item: &'a T },
    Update { index: usize, item: &'a T },
    Remove { index: usize },
    Reset { items: &'a [T] },
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for ListDiff<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Insert { index, item } => DiffRepr::Insert {
                index: *index,
                item,
            },
            Self::Update { index, item } => DiffRepr::Update {
                index: *index,
                item,
            },
            Self::Remove { index } => DiffRepr::Remove { index: *index },
            Self::Reset(items) => DiffRepr::Reset { items },
        }
        .serialize(serializer)
    }
}

/// An item of a view. The key identifies the same item across two versions of the list.
pub trait ViewItem: Clone + PartialEq {
    type Key: Eq + Hash + Clone;
    fn key(&self) -> Self::Key;
}

/// The diffs that turn `old` into `new`. Keys must be unique in each list.
pub fn diff<T: ViewItem>(old: &[T], new: &[T]) -> Vec<ListDiff<T>> {
    let new_keys: HashMap<T::Key, usize> = new
        .iter()
        .enumerate()
        .map(|(i, item)| (item.key(), i))
        .collect();
    let mut out = Vec::new();

    // 1. Remove the items that are gone, from the back, so earlier indexes stay valid.
    let mut current: Vec<T> = old.to_vec();
    for index in (0..current.len()).rev() {
        if !new_keys.contains_key(&current[index].key()) {
            current.remove(index);
            out.push(ListDiff::Remove { index });
        }
    }

    // 2. Walk the new list. Each position either matches, moves, or is new.
    for (index, item) in new.iter().enumerate() {
        let key = item.key();
        match current.get(index) {
            Some(existing) if existing.key() == key => {
                if existing != item {
                    current[index] = item.clone();
                    out.push(ListDiff::Update {
                        index,
                        item: item.clone(),
                    });
                }
            }
            _ => {
                if let Some(from) = current.iter().position(|c| c.key() == key) {
                    // A move: remove it from its old place first.
                    current.remove(from);
                    out.push(ListDiff::Remove { index: from });
                }
                current.insert(index, item.clone());
                out.push(ListDiff::Insert {
                    index,
                    item: item.clone(),
                });
            }
        }
    }
    out
}

/// Apply diffs to a list. Frontends do the same. Also used by the tests.
pub fn apply<T: Clone>(list: &mut Vec<T>, diffs: &[ListDiff<T>]) {
    for d in diffs {
        match d {
            ListDiff::Insert { index, item } => list.insert(*index, item.clone()),
            ListDiff::Update { index, item } => list[*index] = item.clone(),
            ListDiff::Remove { index } => {
                list.remove(*index);
            }
            ListDiff::Reset(items) => *list = items.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct Item(u32, &'static str);

    impl ViewItem for Item {
        type Key = u32;
        fn key(&self) -> u32 {
            self.0
        }
    }

    fn check(old: Vec<Item>, new: Vec<Item>) -> Vec<ListDiff<Item>> {
        let diffs = diff(&old, &new);
        let mut list = old;
        apply(&mut list, &diffs);
        assert_eq!(list, new, "diffs: {diffs:?}");
        diffs
    }

    #[test]
    fn append_is_one_insert() {
        let diffs = check(vec![Item(1, "a")], vec![Item(1, "a"), Item(2, "b")]);
        assert_eq!(
            diffs,
            vec![ListDiff::Insert {
                index: 1,
                item: Item(2, "b")
            }]
        );
    }

    #[test]
    fn prepend_is_one_insert_at_zero() {
        let diffs = check(vec![Item(2, "b")], vec![Item(1, "a"), Item(2, "b")]);
        assert_eq!(
            diffs,
            vec![ListDiff::Insert {
                index: 0,
                item: Item(1, "a")
            }]
        );
    }

    #[test]
    fn changed_item_is_one_update() {
        let diffs = check(
            vec![Item(1, "a"), Item(2, "b")],
            vec![Item(1, "a"), Item(2, "B")],
        );
        assert_eq!(
            diffs,
            vec![ListDiff::Update {
                index: 1,
                item: Item(2, "B")
            }]
        );
    }

    #[test]
    fn same_list_is_no_diff() {
        assert!(check(vec![Item(1, "a")], vec![Item(1, "a")]).is_empty());
    }

    #[test]
    fn removes_moves_and_inserts_combine() {
        check(
            vec![Item(1, "a"), Item(2, "b"), Item(3, "c"), Item(4, "d")],
            vec![Item(4, "d"), Item(5, "e"), Item(2, "B"), Item(1, "a")],
        );
        check(vec![], vec![Item(1, "a"), Item(2, "b")]);
        check(vec![Item(1, "a"), Item(2, "b")], vec![]);
    }

    #[test]
    fn many_random_pairs_round_trip() {
        // A small deterministic generator, so the test needs no crate.
        let mut seed = 0x2545_F491_u64;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for _ in 0..500 {
            let mut make = |n: u64| -> Vec<Item> {
                let mut keys: Vec<u32> = (0..10).collect();
                let len = (next() % n) as usize;
                let mut out = Vec::new();
                for _ in 0..len.min(keys.len()) {
                    let k = keys.remove((next() % keys.len() as u64) as usize);
                    out.push(Item(k, if next() % 2 == 0 { "x" } else { "y" }));
                }
                out
            };
            let (old, new) = (make(10), make(10));
            check(old, new);
        }
    }
}
