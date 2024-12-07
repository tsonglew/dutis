use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct BiMap<K, V> {
    forward: HashMap<K, V>,
    reverse: HashMap<V, K>,
}

impl<K, V> BiMap<K, V>
where
    K: Clone + Eq + std::hash::Hash,
    V: Clone + Eq + std::hash::Hash,
{
    pub fn new() -> Self {
        BiMap {
            forward: HashMap::new(),
            reverse: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<(K, V)> {
        // Remove any existing mappings
        let old_key = self.remove_by_value(&value);
        let old_value = self.remove_by_key(&key);

        self.forward.insert(key.clone(), value.clone());
        self.reverse.insert(value, key);

        match (old_key, old_value) {
            (Some(k), Some(v)) => Some((k, v)),
            _ => None,
        }
    }

    pub fn remove_by_key(&mut self, key: &K) -> Option<V> {
        if let Some(value) = self.forward.remove(key) {
            self.reverse.remove(&value);
            Some(value)
        } else {
            None
        }
    }

    pub fn remove_by_value(&mut self, value: &V) -> Option<K> {
        if let Some(key) = self.reverse.remove(value) {
            self.forward.remove(&key);
            Some(key)
        } else {
            None
        }
    }

    pub fn get_by_key(&self, key: &K) -> Option<&V> {
        self.forward.get(key)
    }

    pub fn get_by_value(&self, value: &V) -> Option<&K> {
        self.reverse.get(value)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.forward.contains_key(key)
    }

    pub fn contains_value(&self, value: &V) -> bool {
        self.reverse.contains_key(value)
    }

    pub fn len(&self) -> usize {
        self.forward.len()
    }

    pub fn is_empty(&self) -> bool {
        self.forward.is_empty()
    }

    pub fn clear(&mut self) {
        self.forward.clear();
        self.reverse.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.forward.iter()
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.forward.keys()
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.forward.values()
    }
}
