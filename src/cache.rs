use std::collections::HashMap;
use std::hash::Hash;

pub struct LruCache<K, V> {
    entries: HashMap<K, Entry<V>>,
    byte_limit: usize,
    bytes_used: usize,
    clock: u64,
}

struct Entry<V> {
    value: V,
    bytes: usize,
    last_used: u64,
}

impl<K: Eq + Hash + Clone, V> LruCache<K, V> {
    pub fn new(byte_limit: usize) -> Self {
        Self {
            entries: HashMap::new(),
            byte_limit,
            bytes_used: 0,
            clock: 0,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.clock = self.clock.wrapping_add(1);
        let entry = self.entries.get_mut(key)?;
        entry.last_used = self.clock;
        Some(&entry.value)
    }

    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        if bytes > self.byte_limit {
            return;
        }

        if let Some(old) = self.entries.remove(&key) {
            self.bytes_used -= old.bytes;
        }

        while bytes > self.byte_limit - self.bytes_used {
            let Some(oldest_key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };

            if let Some(oldest) = self.entries.remove(&oldest_key) {
                self.bytes_used -= oldest.bytes;
            }
        }

        self.clock = self.clock.wrapping_add(1);
        self.bytes_used += bytes;
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                last_used: self.clock,
            },
        );
    }

    pub fn bytes_used(&self) -> usize {
        self.bytes_used
    }

    pub fn snapshot(&self) -> Vec<(K, V)>
    where
        K: Clone,
        V: Clone,
    {
        self.entries
            .iter()
            .map(|(key, entry)| (key.clone(), entry.value.clone()))
            .collect()
    }

}

#[cfg(test)]
mod tests {
    use super::LruCache;

    #[test]
    fn evicts_least_recently_used_entry_to_respect_byte_limit() {
        let mut cache = LruCache::new(9);
        cache.insert("first", 1, 4);
        cache.insert("second", 2, 4);
        assert_eq!(cache.get(&"first"), Some(&1));

        cache.insert("third", 3, 5);

        assert_eq!(cache.bytes_used(), 9);
        assert_eq!(cache.get(&"first"), Some(&1));
        assert_eq!(cache.get(&"second"), None);
        assert_eq!(cache.get(&"third"), Some(&3));
    }

    #[test]
    fn ignores_entries_larger_than_the_entire_budget() {
        let mut cache = LruCache::new(4);
        cache.insert("oversized", 1, 5);

        assert_eq!(cache.bytes_used(), 0);
        assert_eq!(cache.get(&"oversized"), None);
    }

    #[test]
    fn replacing_an_entry_updates_its_accounted_size() {
        let mut cache = LruCache::new(8);
        cache.insert("page", 1, 4);
        cache.insert("page", 2, 6);

        assert_eq!(cache.bytes_used(), 6);
        assert_eq!(cache.get(&"page"), Some(&2));
    }
}
