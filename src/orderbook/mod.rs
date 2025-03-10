use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod orderbook;

type Price = i64;
type Quantity = u64;

struct LevelInfo {
    price: Price,
    quantity: Quantity,
}

struct OrderbookLevelInfo {
    bids: Vec<LevelInfo>,
    asks: Vec<LevelInfo>,
}

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

    fn get_filled_quantity(&self) -> Quantity {
        self.initial_quantity - self.remaining_quantity
    }

    fn fill(&mut self, quantity: Quantity) -> Result<()> {
        if quantity > self.remaining_quantity {
            return Err(anyhow!(
                "Order cannot be filled for more that its remaining quantity"
            ));
        }

        self.remaining_quantity -= quantity;

        Ok(())
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

struct OrderModify {
    order_id: Uuid,
    side: OrderSide,
    price: Price,
    quantity: Quantity,
}

#[derive(Debug)]
struct TradeInfo {
    order_id: Uuid,
    price: Price,
    quantity: Quantity,
}

impl From<(Order, Quantity)> for TradeInfo {
    fn from(value: (Order, Quantity)) -> Self {
        let order = value.0;
        let quantity = value.1;
        Self {
            order_id: order.id,
            price: order.price,
            quantity,
        }
    }
}

/// matched order, aggregate of bid and ask
#[derive(Debug)]
pub struct Trade {
    bid: TradeInfo,
    ask: TradeInfo,
}
