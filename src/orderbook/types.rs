use std::{
    cmp::{min, Reverse},
    collections::{BTreeMap, HashMap, VecDeque},
};

use anyhow::{anyhow, Context, Result};
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
    asks: BTreeMap<Price, VecDeque<Order>>,
    bids: BTreeMap<Reverse<Price>, VecDeque<Order>>,
    orders: HashMap<Uuid, Order>,
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

    fn process_trade(bid: &mut Order, ask: &mut Order) -> Result<Option<Trade>> {
        if ask.price > bid.price {
            return Ok(None);
        }

        let quantity = min(ask.remaining_quantity, bid.remaining_quantity);
        bid.fill(quantity)?;
        ask.fill(quantity)?;

        let trade = Trade {
            bid: (*bid, quantity).into(),
            ask: (*ask, quantity).into(),
        };

        Ok(Some(trade))
    }

    fn match_orders(&mut self) -> Result<Vec<Trade>> {
        let mut trades = vec![];

        loop {
            if self.asks.is_empty() || self.bids.is_empty() {
                break;
            }

            match (self.asks.first_entry(), self.bids.first_entry()) {
                (Some(mut asks_entry), Some(mut bids_entry)) => {
                    let bids = bids_entry.get_mut();
                    let asks = asks_entry.get_mut();
                    let bid = bids.get_mut(0).context("Should have first")?;
                    let ask = asks.get_mut(0).context("Should have first")?;

                    match Orderbook::process_trade(bid, ask)? {
                        Some(trade) => trades.push(trade),
                        None => break,
                    }

                    // if bid or ask completely filled, remove it
                    if bid.remaining_quantity == 0 {
                        self.orders.remove(&bid.id);
                        let _ = bids.pop_front();
                    }
                    if ask.remaining_quantity == 0 {
                        self.orders.remove(&ask.id);
                        let _ = asks.pop_front();
                    }

                    // if not more bids or asks at currently level, remove level
                    if bids.is_empty() {
                        let _ = bids_entry.remove_entry();
                    }
                    if asks.is_empty() {
                        let _ = asks_entry.remove_entry();
                    }
                }
                _ => break,
            }
        }

        Ok(trades)
    }
}
