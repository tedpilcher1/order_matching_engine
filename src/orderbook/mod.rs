use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod order_levels;
pub mod orderbook;

type Price = i64;
type Quantity = u64;

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Order {
    pub type_: OrderType,
    pub id: Uuid,
    pub side: OrderSide,
    pub price: Price,
    pub initial_quantity: Quantity,
    pub remaining_quantity: Quantity,
}

impl Order {
    pub fn new(type_: OrderType, side: OrderSide, price: Price, quantity: Quantity) -> Self {
        Self {
            type_,
            id: Uuid::new_v4(),
            side,
            price,
            initial_quantity: quantity,
            remaining_quantity: quantity,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Deserialize, Serialize)]
pub enum OrderType {
    Normal,
    Kill,
}

#[derive(PartialEq, Clone, Copy, Debug, Deserialize, Serialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, PartialEq)]
struct TradeInfo {
    order_id: Uuid,
    price: Price,
    quantity: Quantity,
}

/// matched order, aggregate of bid and ask
#[derive(Debug, PartialEq)]
pub struct Trade {
    bid: TradeInfo,
    ask: TradeInfo,
}

// TODO: Simplify
#[derive(Debug)]
pub enum ProcessTradeError {
    MinQuantityNotMet(Vec<MinQuantityNotMetTypes>),
    PriceDiscrepancy,
    FillQuantityHigherThanRemaining,
}

#[derive(Debug)]
pub enum MinQuantityNotMetTypes {
    Ask,
    Bid,
}
