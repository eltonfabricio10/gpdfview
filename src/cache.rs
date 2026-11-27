use std::collections::HashMap;
use std::hash::Hash;

pub struct LruCache<K, V> {
    entries: HashMap<K, Entry<V>>,
    byte_limit: usize,
    bytes_used: usize,
    clock: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
    rejected: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub rejected: u64,
    pub bytes_used: usize,
    pub entries: usize,
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
            hits: 0,
            misses: 0,
            evictions: 0,
            rejected: 0,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.clock = self.clock.wrapping_add(1);
        let Some(entry) = self.entries.get_mut(key) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        self.hits = self.hits.saturating_add(1);
        entry.last_used = self.clock;
        Some(&entry.value)
    }

    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        if bytes > self.byte_limit {
            self.rejected = self.rejected.saturating_add(1);
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
                self.evictions = self.evictions.saturating_add(1);
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

    pub fn stats(&self) -> CacheStats {
        CacheStats {
            hits: self.hits,
            misses: self.misses,
            evictions: self.evictions,
            rejected: self.rejected,
            bytes_used: self.bytes_used,
            entries: self.entries.len(),
        }
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
    fn cache_stats_track_hits_misses_evictions_and_rejections() {
        let mut cache = LruCache::new(5);
        assert_eq!(cache.get(&"missing"), None);
        cache.insert("first", 1, 3);
        assert_eq!(cache.get(&"first"), Some(&1));
        cache.insert("second", 2, 3);
        cache.insert("oversized", 3, 6);

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.evictions, 1);
        assert_eq!(stats.rejected, 1);
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.bytes_used, 3);
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
