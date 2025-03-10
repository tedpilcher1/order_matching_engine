use std::collections::{BTreeMap, VecDeque};

use uuid::Uuid;

use super::Price;

pub trait OrderLevels {
    fn new() -> Self;
    fn insert_order(&mut self, price: Price, order_id: Uuid);
    fn remove_order(&mut self, price: &Price, order_id: &Uuid) -> bool;
    fn get_order(&self, price: Price, offset: usize) -> Option<&Uuid>;
    fn get_prices(&self) -> Vec<&Price>;
}

struct GenericOrderLevels<K> {
    levels: BTreeMap<K, VecDeque<Uuid>>,
}

impl<K> GenericOrderLevels<K>
where
    K: Ord,
{
    fn new() -> Self {
        Self {
            levels: BTreeMap::new(),
        }
    }

    fn insert_order(&mut self, key: K, order_id: Uuid) {
        self.levels
            .entry(key)
            .or_insert_with(VecDeque::new)
            .push_back(order_id);
    }

    fn remove_order(&mut self, key: &K, order_id: &Uuid) -> bool {
        if let Some(orders) = self.levels.get_mut(key) {
            if let Some(index) = orders.iter().position(|x| x == order_id) {
                let _ = orders.remove(index);
                return true;
            }
        }
        false
    }

    fn get_order(&self, key: K, offset: usize) -> Option<&Uuid> {
        self.levels.get(&key).and_then(|orders| orders.get(offset))
    }

    fn get_prices(&self) -> Vec<&K> {
        self.levels.keys().collect()
    }
}
