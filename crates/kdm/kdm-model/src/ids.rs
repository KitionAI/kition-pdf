//! Overlay keys and this replica's object numbers.
//!
//! An object of the base keeps its number: its overlay key is the number in decimal (`"12"`).
//! An object added since the base was written gets a key of its own, `<actor>.<n>` (the adding
//! replica's actor id in hex and a counter), so two peers adding objects at once never take the
//! same key. Which *number* such an object has is each replica's own choice, made when it first
//! sees the key (above the base's highest number, never reused within a session): the numbers
//! stay dense and within what PDF readers accept (ISO 32000-1 Annex C's 8,388,607 indirect
//! objects), which per-actor number ranges carved out of that space could not promise to any
//! number of peers. Keys are what replicate; numbers are a view's.
//!
//! The numbering belongs to one epoch (one base): a rebase starts a new one.

use std::collections::{BTreeMap, HashMap};

/// The numbering of one epoch's added objects.
#[derive(Clone, Debug, Default)]
struct Numbering {
    by_num: BTreeMap<u32, String>,
    by_key: HashMap<String, u32>,
}

/// Overlay keys ↔ object numbers for the view of one replica. See the module docs.
#[derive(Clone, Debug, Default)]
pub struct Ids {
    actor: u64,
    counter: u64,
    epoch: String,
    /// The base's highest object number: numbers up to it are the base's own.
    base_max: u32,
    current: Numbering,
    /// Other epochs' numberings (an undo back across a rebase finds its numbers again).
    others: HashMap<String, Numbering>,
    /// Numbers [`Ids::key_of`] gave a key since the last [`Ids::take_minted`].
    minted: Vec<u32>,
}

/// Whether `key` names an object of the base (a decimal number), and which.
pub fn base_number(key: &str) -> Option<u32> {
    if key.is_empty() || key.len() > 10 || !key.bytes().all(|b| b.is_ascii_digit()) || (key.len() > 1 && key.starts_with('0')) {
        return None;
    }
    key.parse().ok()
}

impl Ids {
    pub fn new(actor: u64) -> Self {
        Self { actor, ..Self::default() }
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub fn base_max(&self) -> u32 {
        self.base_max
    }

    /// Switches to `epoch` (whose base's highest number is `base_max`), keeping the numbering of
    /// the one it leaves for a later return.
    pub fn enter(&mut self, epoch: &str, base_max: u32) {
        if self.epoch != epoch {
            let left = std::mem::take(&mut self.current);
            let previous = std::mem::replace(&mut self.epoch, epoch.to_owned());
            self.others.insert(previous, left);
            self.current = self.others.remove(epoch).unwrap_or_default();
        }
        self.base_max = base_max;
    }

    /// The overlay key of object `num`, minting one for a number added since the base.
    pub fn key_of(&mut self, num: u32) -> String {
        if num <= self.base_max {
            return num.to_string();
        }
        if let Some(key) = self.current.by_num.get(&num) {
            return key.clone();
        }
        let key = format!("{:x}.{}", self.actor, self.counter);
        self.counter += 1;
        self.bind(&key, num);
        self.minted.push(num);
        key
    }

    /// The overlay key of `num` if it has one (never mints).
    pub fn known_key(&self, num: u32) -> Option<String> {
        if num <= self.base_max { Some(num.to_string()) } else { self.current.by_num.get(&num).cloned() }
    }

    /// This replica's number for `key`, giving an added object it has not seen yet the next
    /// number at or above `floor` that nothing uses. `None` for a malformed key.
    pub fn num_of(&mut self, key: &str, floor: u32) -> Option<u32> {
        if let Some(num) = base_number(key) {
            return Some(num);
        }
        if key.is_empty() || key.len() > 64 {
            return None;
        }
        if let Some(num) = self.current.by_key.get(key) {
            return Some(*num);
        }
        let num = self.next_free(floor);
        self.bind(key, num);
        Some(num)
    }

    /// The number after every number this epoch's numbering has handed out, and at least
    /// `floor` and past the base.
    pub fn next_free(&self, floor: u32) -> u32 {
        let after = self.current.by_num.keys().next_back().map_or(0, |n| n.saturating_add(1));
        after.max(floor).max(self.base_max.saturating_add(1))
    }

    /// Binds `key` to `num` (a renumbering when a host reopens a saved file as the view).
    pub fn bind(&mut self, key: &str, num: u32) {
        if let Some(old) = self.current.by_key.insert(key.to_owned(), num) {
            self.current.by_num.remove(&old);
        }
        self.current.by_num.insert(num, key.to_owned());
    }

    /// The added objects' numbers, by key.
    pub fn added(&self) -> impl Iterator<Item = (&str, u32)> {
        self.current.by_key.iter().map(|(key, num)| (key.as_str(), *num))
    }

    /// The numbers given a key since the last call (a write checks that it wrote them).
    pub(crate) fn take_minted(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.minted)
    }

    /// Forgets this epoch's numbering of added objects (before a renumbering).
    pub(crate) fn clear_added(&mut self) {
        self.current = Numbering::default();
    }
}
