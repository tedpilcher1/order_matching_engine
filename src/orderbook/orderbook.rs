use std::{
    cmp::{min, Reverse},
    collections::{BTreeMap, HashMap, VecDeque},
};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::metrics::{
    BUY_ORDER_PRICE, MATCHING_DURATION, ORDERS_FILLED_COUNTER, ORDER_COUNTER, SELL_ORDER_PRICE,
    TRADE_COUNTER,
};

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

/// Map to reresents bids and asks
/// bids desc (first/highest is best buy price), asks asc (first/lowest is best sell price)
#[derive(Debug)]
pub struct Orderbook {
    asks: BTreeMap<Price, VecDeque<Order>>,
    bids: BTreeMap<Reverse<Price>, VecDeque<Order>>,
    orders: HashMap<Uuid, Order>,
}

// TODO: Check if can match before running matching algorithm
impl Orderbook {
    pub fn new() -> Self {
        Self {
            asks: BTreeMap::new(),
            bids: BTreeMap::new(),
            orders: HashMap::new(),
        }
    }

    pub fn cancel_order(&mut self, order_id: Uuid) -> Result<bool> {
        match self.orders.remove(&order_id) {
            Some(order) => match &order.side {
                OrderSide::Buy => {
                    if let Some(bids) = self.bids.get_mut(&Reverse(order.price)) {
                        bids.retain(|&x| x != order);
                        if bids.is_empty() {
                            self.bids.remove(&Reverse(order.price));
                        }
                    }

                    Ok(true)
                }
                OrderSide::Sell => {
                    if let Some(asks) = self.asks.get_mut(&order.price) {
                        asks.retain(|&x| x != order);

                        if asks.is_empty() {
                            self.asks.remove(&order.price);
                        }
                    }

                    Ok(true)
                }
            },
            None => Ok(false),
        }
    }

    pub fn add_order(&mut self, order: Order) -> Result<Vec<Trade>> {
        ORDER_COUNTER.inc();

        match order.side {
            OrderSide::Buy => {
                for _ in 0..order.initial_quantity {
                    BUY_ORDER_PRICE.observe(order.price as f64);
                }
            }
            OrderSide::Sell => {
                for _ in 0..order.initial_quantity {
                    SELL_ORDER_PRICE.observe(order.price as f64);
                }
            }
        }

        let start_time = Utc::now();

        if self.orders.contains_key(&order.id) {
            return Err(anyhow!("Order id already exists "));
        }

        match &order.side {
            OrderSide::Buy => {
                self.bids
                    .entry(Reverse(order.price))
                    .or_insert_with(VecDeque::new)
                    .push_back(order);
            }
            OrderSide::Sell => {
                self.asks
                    .entry(order.price)
                    .or_insert_with(VecDeque::new)
                    .push_back(order);
            }
        }

        self.orders.insert(order.id, order);

        let res = self.match_orders();

        let end_time = Utc::now();

        MATCHING_DURATION.observe((end_time - start_time).num_seconds() as f64);

        // println!("ADDED ORDER");

        res
    }

    fn can_match(&mut self, side: &OrderSide, price: &Price) -> bool {
        match side {
            OrderSide::Buy => match self.asks.first_key_value() {
                Some((best_ask, _)) => price >= best_ask,
                None => false,
            },
            OrderSide::Sell => match self.bids.first_key_value() {
                Some((Reverse(best_bid), _)) => price <= best_bid,
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

        TRADE_COUNTER.inc();
        // println!("PROCESSED TRADE");

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
                        ORDERS_FILLED_COUNTER.inc();
                        self.orders.remove(&bid.id);
                        let _ = bids.pop_front();
                    }
                    if ask.remaining_quantity == 0 {
                        ORDERS_FILLED_COUNTER.inc();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_empty_orderbook(orderbook: &Orderbook) {
        assert!(orderbook.bids.is_empty());
        assert!(orderbook.asks.is_empty())
    }

    #[test]
    fn basic_order_match() {
        let mut orderbook = Orderbook::new();
        let price = 10;
        let quantity = 1;

        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, quantity);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, price, quantity);

        let first_trades = orderbook.add_order(buy_order).unwrap();
        let second_trades = orderbook.add_order(sell_order).unwrap();

        assert!(first_trades.is_empty());

        let trade = second_trades.first().unwrap();
        assert_eq!(trade.ask.price, price);
        assert_eq!(trade.ask.quantity, quantity);

        assert_eq!(trade.bid.price, price);
        assert_eq!(trade.bid.quantity, quantity);

        assert_empty_orderbook(&orderbook)
    }

    #[test]
    fn partial_order_match() {
        let mut orderbook = Orderbook::new();
        let price = 10;
        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, 5);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, price, 10);

        let first_trades = orderbook.add_order(buy_order).unwrap();
        let second_trades = orderbook.add_order(sell_order).unwrap();

        assert!(first_trades.is_empty());

        let trade = second_trades.first().unwrap();
        assert_eq!(trade.ask.price, price);
        assert_eq!(trade.ask.quantity, 5);

        assert_eq!(trade.bid.price, price);
        assert_eq!(trade.bid.quantity, 5);

        assert!(orderbook.bids.is_empty());
        assert!(!orderbook.asks.is_empty());
    }
}
