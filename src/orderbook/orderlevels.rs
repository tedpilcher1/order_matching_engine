use std::{
    cmp::Reverse,
    collections::{BTreeMap, VecDeque},
};

use uuid::Uuid;

use super::Price;

pub trait OrderLevels {
    fn new() -> Self;
    fn insert_order(&mut self, price: Price, order_id: Uuid);
    fn remove_order(&mut self, price: &Price, order_id: &Uuid) -> bool;
    fn get_order(&self, price: Price, offset: usize) -> Option<&Uuid>;
    fn get_prices(&self) -> Vec<&Price>;
    fn get_best_price(&self) -> Option<&Price>;
    fn get_orders(&self, price: &Price) -> Option<&VecDeque<Uuid>>;
    fn remove_empty_levels(&mut self);
}

#[derive(Debug)]
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
            .or_default()
            .push_back(order_id);
    }

    fn remove_order(&mut self, key: &K, order_id: &Uuid) -> bool {
        if let Some(orders) = self.levels.get_mut(key) {
            if let Some(index) = orders.iter().position(|x| x == order_id) {
                orders.remove(index);
                if orders.is_empty() {
                    self.levels.remove(key);
                }
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

    fn get_best_price(&self) -> Option<&K> {
        self.levels
            .first_key_value().map(|key_value| key_value.0)
    }

    fn get_orders(&self, key: &K) -> Option<&VecDeque<Uuid>> {
        self.levels.get(key)
    }
}

#[derive(Debug)]
pub struct AskOrderLevels {
    inner: GenericOrderLevels<Price>,
}

impl OrderLevels for AskOrderLevels {
    fn new() -> Self {
        Self {
            inner: GenericOrderLevels::new(),
        }
    }

    fn insert_order(&mut self, price: Price, order_id: Uuid) {
        self.inner.insert_order(price, order_id);
    }

    fn remove_order(&mut self, price: &Price, order_id: &Uuid) -> bool {
        self.inner.remove_order(price, order_id)
    }

    fn get_order(&self, price: Price, offset: usize) -> Option<&Uuid> {
        self.inner.get_order(price, offset)
    }

    fn get_prices(&self) -> Vec<&Price> {
        self.inner.get_prices()
    }

    fn get_best_price(&self) -> Option<&Price> {
        self.inner.get_best_price()
    }

    fn get_orders(&self, price: &Price) -> Option<&VecDeque<Uuid>> {
        self.inner.get_orders(price)
    }

    fn remove_empty_levels(&mut self) {
        self.inner.levels.retain(|_, orders| !orders.is_empty());
    }
}

#[derive(Debug)]
pub struct BidOrderLevels {
    inner: GenericOrderLevels<Reverse<Price>>,
}

impl OrderLevels for BidOrderLevels {
    fn new() -> Self {
        Self {
            inner: GenericOrderLevels::new(),
        }
    }

    fn insert_order(&mut self, price: Price, order_id: Uuid) {
        self.inner.insert_order(Reverse(price), order_id);
    }

    fn remove_order(&mut self, price: &Price, order_id: &Uuid) -> bool {
        self.inner.remove_order(&Reverse(*price), order_id)
    }

    fn get_order(&self, price: Price, offset: usize) -> Option<&Uuid> {
        self.inner.get_order(Reverse(price), offset)
    }

    fn get_prices(&self) -> Vec<&Price> {
        self.inner
            .get_prices()
            .into_iter()
            .map(|reverse_price| &reverse_price.0)
            .collect()
    }

    fn get_best_price(&self) -> Option<&Price> {
        self.inner
            .get_best_price()
            .map(|reverse_price| &reverse_price.0)
    }

    fn get_orders(&self, price: &Price) -> Option<&VecDeque<Uuid>> {
        self.inner.get_orders(&Reverse(*price))
    }

    fn remove_empty_levels(&mut self) {
        self.inner.levels.retain(|_, orders| !orders.is_empty());
    }
}
