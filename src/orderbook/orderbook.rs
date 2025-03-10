use std::{
    cmp::{min, Reverse},
    collections::{BTreeMap, HashMap, VecDeque},
};

use anyhow::{anyhow, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::metrics::{
    BUY_ORDER_PRICE, MATCHING_DURATION, ORDER_COUNTER, SELL_ORDER_PRICE, TRADE_COUNTER,
};

use super::{Order, OrderSide, OrderType, Price, ProcessTradeError, Trade};

/// Map to reresents bids and asks
/// bids desc (first/highest is best buy price), asks asc (first/lowest is best sell price)
#[derive(Debug)]
pub struct Orderbook {
    ask_levels: BTreeMap<Price, VecDeque<Uuid>>,
    bid_levels: BTreeMap<Reverse<Price>, VecDeque<Uuid>>,
    bid_orders: HashMap<Uuid, Order>,
    ask_orders: HashMap<Uuid, Order>,
}

impl Orderbook {
    pub fn new() -> Self {
        Self {
            ask_levels: BTreeMap::new(),
            bid_levels: BTreeMap::new(),
            bid_orders: HashMap::new(),
            ask_orders: HashMap::new(),
        }
    }

    /// Modifies an order, equivalent to cancel + add
    ///
    /// Cannot modify an order to a new type or side
    ///
    /// Doesn't modify in place, cancels, and adds new order
    ///
    /// Quantity of new order is abs(modified_new_order - old_order)
    pub fn modify_order(&mut self, order: Order) -> Result<()> {
        let existing_order = match &order.side {
            OrderSide::Buy => self.bid_orders.get(&order.id),
            OrderSide::Sell => self.ask_orders.get(&order.id),
        };

        match existing_order {
            Some(existing_order) => {
                if existing_order.type_ != order.type_ {
                    return Ok(());
                }

                if let Ok(Some(cancelled_order)) = self.cancel_order(order.id) {
                    let remaining_quantity = order
                        .remaining_quantity
                        .abs_diff(cancelled_order.remaining_quantity);

                    let fresh_order = Order {
                        type_: order.type_,
                        id: order.id,
                        side: order.side,
                        price: order.price,
                        initial_quantity: remaining_quantity,
                        remaining_quantity,
                        minimum_quantity: cancelled_order.minimum_quantity,
                    };

                    let _ = self.add_order(fresh_order);
                }
            }
            // cannot modify side
            None => return Ok(()),
        }

        Ok(())
    }

    pub fn cancel_order(&mut self, order_id: Uuid) -> Result<Option<Order>> {
        if let Some(order) = self.bid_orders.get(&order_id) {
            if let Some(bid_levels) = self.bid_levels.get_mut(&Reverse(order.price)) {
                bid_levels.retain(|&x| x != order.id);
                if bid_levels.is_empty() {
                    self.bid_levels.remove(&Reverse(order.price));
                    return Ok(Some(*order));
                }
            }
        }

        if let Some(order) = self.ask_orders.get(&order_id) {
            if let Some(ask_levels) = self.ask_levels.get_mut(&order.price) {
                ask_levels.retain(|&x| x != order.id);
                if ask_levels.is_empty() {
                    self.ask_levels.remove(&order.price);
                    return Ok(Some(*order));
                }
            }
        }

        Ok(None)
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

        match &order.side {
            OrderSide::Buy => {
                if self.bid_orders.contains_key(&order.id) {
                    return Err(anyhow!("Order id already exists "));
                }

                self.bid_levels
                    .entry(Reverse(order.price))
                    .or_insert_with(VecDeque::new)
                    .push_back(order.id);
                self.bid_orders.insert(order.id, order);
            }
            OrderSide::Sell => {
                if self.ask_orders.contains_key(&order.id) {
                    return Err(anyhow!("Order id already exists "));
                }
                self.ask_levels
                    .entry(order.price)
                    .or_insert_with(VecDeque::new)
                    .push_back(order.id);
                self.ask_orders.insert(order.id, order);
            }
        }

        let res = match self.can_match(&order.side, &order.price) {
            true => {
                let start_time = Utc::now();
                let res = self.match_orders();
                let end_time = Utc::now();
                MATCHING_DURATION.observe((end_time - start_time).num_seconds() as f64);
                res?
            }
            false => vec![],
        };

        self.handle_order_type(&order.type_, &order.id)?;

        Ok(res)
    }

    fn handle_order_type(&mut self, order_type: &OrderType, order_id: &Uuid) -> Result<()> {
        match order_type {
            OrderType::Kill => {
                let _ = self.cancel_order(*order_id)?;
            }
            OrderType::Normal => {}
        }

        Ok(())
    }

    fn can_match(&mut self, side: &OrderSide, price: &Price) -> bool {
        match side {
            OrderSide::Buy => match self.ask_levels.first_key_value() {
                Some((best_ask, _)) => price >= best_ask,
                None => false,
            },
            OrderSide::Sell => match self.bid_levels.first_key_value() {
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
            if self.ask_levels.is_empty() || self.bid_levels.is_empty() {
                break;
            }

            match (self.ask_levels.first_entry(), self.bid_levels.first_entry()) {
                (Some(mut asks_entry), Some(mut bids_entry)) => {
                    let bids = bids_entry.get_mut();
                    let asks = asks_entry.get_mut();
                    let bid_id = bids.get_mut(0).context("Should have first")?;
                    let ask_id = asks.get_mut(0).context("Should have first")?;
                    let bid = self
                        .bid_orders
                        .get_mut(&bid_id)
                        .context("Bid should be stored")?;
                    let ask = self
                        .ask_orders
                        .get_mut(&ask_id)
                        .context("Ask should be stored")?;

                    match Orderbook::process_trade(bid, ask)? {
                        Some(trade) => trades.push(trade),
                        None => break,
                    }

                    // if bid or ask completely filled, remove it
                    if bid.remaining_quantity == 0 {
                        ORDERS_FILLED_COUNTER.inc();
                        self.bid_orders.remove(&bid_id);
                        let _ = bids.pop_front();
                    }
                    if ask.remaining_quantity == 0 {
                        ORDERS_FILLED_COUNTER.inc();
                        self.ask_orders.remove(ask_id);
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
        assert!(orderbook.bid_levels.is_empty());
        assert!(orderbook.ask_levels.is_empty())
    }

    #[test]
    fn basic_order_match() {
        let mut orderbook = Orderbook::new();
        let price = 10;
        let quantity = 1;

        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, quantity, 0).unwrap();
        let sell_order =
            Order::new(OrderType::Normal, OrderSide::Sell, price, quantity, 0).unwrap();

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
        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, 5, 0).unwrap();
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, price, 10, 0).unwrap();

        let first_trades = orderbook.add_order(buy_order).unwrap();
        let second_trades = orderbook.add_order(sell_order).unwrap();

        assert!(first_trades.is_empty());

        let trade = second_trades.first().unwrap();
        assert_eq!(trade.ask.price, price);
        assert_eq!(trade.ask.quantity, 5);

        assert_eq!(trade.bid.price, price);
        assert_eq!(trade.bid.quantity, 5);

        assert!(orderbook.bid_levels.is_empty());
        assert!(!orderbook.ask_levels.is_empty());
    }

    #[test]
    fn fill_or_kill_order() {
        let mut orderbook = Orderbook::new();
        let order = Order::new(OrderType::Kill, OrderSide::Buy, 1, 1, 0).unwrap();

        let trades = orderbook.add_order(order).unwrap();

        assert!(trades.is_empty());

        assert_empty_orderbook(&orderbook)
    }
}
