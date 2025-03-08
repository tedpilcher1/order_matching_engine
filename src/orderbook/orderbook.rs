use std::{
    cmp::{min, Reverse},
    collections::{BTreeMap, HashMap, VecDeque},
};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::metrics::{
    BUY_ORDER_PRICE, MATCHING_DURATION, ORDERS_FILLED_COUNTER, ORDER_COUNTER, SELL_ORDER_PRICE,
    TRADE_COUNTER,
};

use super::{Order, OrderSide, OrderType, Price, Trade};

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

    /// Modifies an order
    ///
    /// Cannot modify an order to a new type or side
    ///
    /// Doesn't modify in place, cancels, and adds new order
    ///
    /// Quantity of new order is abs(modified_new_order - old_order)
    pub fn modify_order(&mut self, order: Order) -> Result<()> {
        // Can't modify order to new type or side
        if let Some(existing_order) = self.orders.get(&order.id) {
            if order.side != existing_order.side || order.type_ != existing_order.type_ {
                return Ok(());
            }
        }

        if let Ok(Some(existing_order)) = self.cancel_order(order.id) {
            let remaining_quantity = order
                .remaining_quantity
                .abs_diff(existing_order.remaining_quantity);

            let fresh_order = Order {
                type_: order.type_,
                id: order.id,
                side: order.side,
                price: order.price,
                initial_quantity: remaining_quantity,
                remaining_quantity,
            };

            let _ = self.add_order(fresh_order);
        }

        Ok(())
    }

    pub fn cancel_order(&mut self, order_id: Uuid) -> Result<Option<Order>> {
        match self.orders.remove(&order_id) {
            Some(order) => match &order.side {
                OrderSide::Buy => {
                    if let Some(bids) = self.bids.get_mut(&Reverse(order.price)) {
                        bids.retain(|&x| x != order);
                        if bids.is_empty() {
                            self.bids.remove(&Reverse(order.price));
                        }
                    }

                    Ok(Some(order))
                }
                OrderSide::Sell => {
                    if let Some(asks) = self.asks.get_mut(&order.price) {
                        asks.retain(|&x| x != order);

                        if asks.is_empty() {
                            self.asks.remove(&order.price);
                        }
                    }

                    Ok(Some(order))
                }
            },
            None => Ok(None),
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

        if order.type_ == OrderType::FillOrKill {
            let _ = self.cancel_order(order.id)?;
        }

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
    use crate::orderbook;

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

    #[test]
    fn fill_or_kill_order() {
        let mut orderbook = Orderbook::new();
        let order = Order::new(OrderType::FillOrKill, OrderSide::Buy, 1, 1);

        let trades = orderbook.add_order(order).unwrap();

        assert!(trades.is_empty());

        assert_empty_orderbook(&orderbook)
    }
}
