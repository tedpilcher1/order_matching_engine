use uuid::Uuid;

use super::Price;

pub trait OrderLevels {
    fn new() -> Self;
    fn insert_order(&mut self, price: Price, order_id: Uuid);
    fn remove_order(&mut self, price: &Price, order_id: &Uuid) -> bool;
    fn get_order(&self, price: Price, offset: usize) -> Option<&Uuid>;
    fn get_prices(&self) -> Vec<&Price>;
}
