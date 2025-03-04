use std::{
    cmp::{min, Reverse},
    collections::BTreeMap,
};

use anyhow::{anyhow, Result};
use uuid::Uuid;

type Price = i64;
type Quantity = u64;

/// Used to get information about state of order book
struct LevelInfo {
    price: Price,
    quantity: Quantity,
}

/// represents each side of order book, each side is list of levels
struct OrderbookLevelInfo {
    bids: Vec<LevelInfo>,
    asks: Vec<LevelInfo>,
}

#[derive(Copy, Clone)]
struct Order {
    type_: OrderType,
    pub id: Uuid,
    side: OrderSide,
    pub price: Price,
    initial_quantity: Quantity,
    remaining_quantity: Quantity,
}

impl Order {
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

#[derive(Copy, Clone)]

enum OrderType {}

#[derive(PartialEq, Clone, Copy)]
enum OrderSide {
    Buy,
    Sell,
}

struct OrderModify {
    order_id: Uuid,
    side: OrderSide,
    price: Price,
    quantity: Quantity,
}

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
struct Trade {
    bid: TradeInfo,
    ask: TradeInfo,
}

/// Map to reresents bids and asks
/// bids desc (first/highest is best buy price), asks asc (first/lowest is best sell price)
struct Orderbook {
    asks: BTreeMap<Price, Vec<Order>>,
    bids: BTreeMap<Reverse<Price>, Vec<Order>>,
    // he includes unordered map of order id -> order (entry) idk the point
}

impl Orderbook {
    fn can_match(&mut self, side: OrderSide, price: Price) -> bool {
        match side {
            OrderSide::Buy => match self.asks.first_key_value() {
                Some((best_ask, _)) => price >= *best_ask,
                None => false,
            },
            OrderSide::Sell => match self.bids.first_key_value() {
                Some((best_bid, _)) => price <= best_bid.0,
                None => false,
            },
        }
    }

    fn match_orders(&mut self) -> Vec<Trade> {
        let mut trades = vec![];

        loop {
            if self.asks.is_empty() || self.bids.is_empty() {
                break;
            }

            match (self.asks.first_entry(), self.bids.first_entry()) {
                (Some(mut asks_entry), Some(mut bids_entry)) => {
                    let asks = asks_entry.get_mut();
                    let bids = bids_entry.get_mut();

                    let ask = asks.get_mut(0).unwrap();
                    let bid = bids.get_mut(0).unwrap();

                    let quantity = min(ask.remaining_quantity, bid.remaining_quantity);
                    let _ = bid.fill(quantity);
                    let _ = ask.fill(quantity);

                    if bid.remaining_quantity == 0 {
                        let _ = bids.remove(0);
                    }

                    if ask.remaining_quantity == 0 {
                        let _ = asks.remove(0);
                    }

                    // if !bids.is_empty() {
                    //     self.bids.remove(bids_entry.key());
                    // }

                    // if !asks.is_empty() {
                    //     self.asks.remove(asks_entry.key());
                    // }

                    trades.push(Trade {
                        bid: (*bid, quantity).into(),
                        ask: (*ask, quantity).into(),
                    });

                    todo!()
                }
                _ => break,
            }
        }

        trades
    }
}
