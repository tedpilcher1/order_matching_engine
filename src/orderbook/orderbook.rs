use std::{cmp::min, collections::HashMap};

use anyhow::{bail, Result};
use uuid::Uuid;

use crate::{metrics::ORDER_COUNTER, web_server::CancelRequestType};

use super::{
    order_levels::{AskOrderLevels, BidOrderLevels, OrderLevels},
    Order, OrderSide, OrderType, Price, Trade, TradeInfo,
};

/// Map to reresents bids and asks
/// bids desc (first/highest is best buy price), asks asc (first/lowest is best sell price)
#[derive(Debug)]
pub struct Orderbook {
    ask_levels: AskOrderLevels,
    bid_levels: BidOrderLevels,
    orders: HashMap<Uuid, Order>,
}

impl Orderbook {
    pub fn new() -> Self {
        Self {
            ask_levels: AskOrderLevels::new(),
            bid_levels: BidOrderLevels::new(),
            orders: HashMap::new(),
        }
    }

    pub fn match_order(&mut self, mut order: Order) -> Result<Vec<Trade>> {
        ORDER_COUNTER.inc();

        if self.orders.contains_key(&order.id) {
            bail!("Order id already in use")
        }

        let trades = match self.can_match_order(&order) {
            true => self.internal_match_order(&mut order),
            false => vec![],
        };

        if order.type_ == OrderType::Normal && order.remaining_quantity > 0 {
            self.insert_order(order)
        }

        Ok(trades)
    }

    fn can_match_order(&self, order: &Order) -> bool {
        match order.side {
            OrderSide::Buy => {
                if let Some(best_opposing_price) = self.ask_levels.get_best_price() {
                    return best_opposing_price <= &order.price;
                }
            }
            OrderSide::Sell => {
                if let Some(best_opposing_price) = self.bid_levels.get_best_price() {
                    return best_opposing_price >= &order.price;
                }
            }
        }
        false
    }

    fn internal_match_order(&mut self, order: &mut Order) -> Vec<Trade> {
        let mut trades = vec![];

        let price_levels = match order.side {
            OrderSide::Buy => self.ask_levels.get_prices(),
            OrderSide::Sell => self.bid_levels.get_prices(),
        };

        for price_level in price_levels {
            // if can't match at price level => break from both loops
            // if order remaining quantity == 0 => break
            if order.remaining_quantity == 0 {
                break;
            }

            let opposing_orders = match order.side {
                OrderSide::Buy => self.ask_levels.get_orders(price_level),
                OrderSide::Sell => self.bid_levels.get_orders(price_level),
            };

            if let Some(opposing_orders) = opposing_orders {
                for opposing_order_id in opposing_orders {
                    if order.remaining_quantity == 0 {
                        break;
                    }

                    let opposing_order = self
                        .orders
                        .get_mut(opposing_order_id)
                        .expect("Order should never be in price level but not in orders");

                    let quantity = min(order.remaining_quantity, opposing_order.remaining_quantity);
                    order.remaining_quantity -= quantity;
                    opposing_order.remaining_quantity -= quantity;

                    let order_trade_info = TradeInfo {
                        order_id: order.id,
                        price: order.price,
                        quantity,
                    };

                    let opposing_order_trade_info = TradeInfo {
                        order_id: *opposing_order_id,
                        price: *price_level,
                        quantity,
                    };

                    let trade = match order.side {
                        OrderSide::Buy => Trade {
                            bid: order_trade_info,
                            ask: opposing_order_trade_info,
                        },
                        OrderSide::Sell => Trade {
                            bid: opposing_order_trade_info,
                            ask: order_trade_info,
                        },
                    };
                    trades.push(trade);
                }
            }
        }
        // remove orders with 0 remaining quantity
        for trade in &trades {
            let opposing_order_id = match order.side {
                OrderSide::Buy => trade.ask.order_id,
                OrderSide::Sell => trade.bid.order_id,
            };

            let opposing_order = self
                .orders
                .get(&opposing_order_id)
                .expect("Order shouldn't have been removed yet");

            // TODO: Can unify removal of order here with cancel order method
            if opposing_order.remaining_quantity == 0 {
                match opposing_order.side {
                    OrderSide::Buy => self
                        .bid_levels
                        .remove_order(&trade.bid.price, &opposing_order_id),
                    OrderSide::Sell => self
                        .ask_levels
                        .remove_order(&trade.ask.price, &opposing_order_id),
                };

                self.orders.remove(&opposing_order_id);
            }
        }
        self.ask_levels.remove_empty_levels();
        self.bid_levels.remove_empty_levels();

        trades
    }

    fn insert_order(&mut self, order: Order) {
        match order.side {
            OrderSide::Buy => self.bid_levels.insert_order(order.price, order.id),
            OrderSide::Sell => self.ask_levels.insert_order(order.price, order.id),
        }
        self.orders.insert(order.id, order);
    }

    /// Modifies an order, equivalent to cancel + add
    ///
    /// Cannot modify an order to a new type or side
    ///
    /// Doesn't modify in place, cancels, and adds new order
    ///
    /// Quantity of new order is abs(modified_new_order - old_order)
    pub fn modify_order(&mut self, order: Order) -> Result<()> {
        let existing_order = self.orders.get(&order.id);

        match existing_order {
            Some(existing_order) => {
                if existing_order.type_ != order.type_ {
                    return Ok(());
                }

                if let Some(cancelled_order) =
                    self.cancel_order(CancelRequestType::Internal, order.id)
                {
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
                    };

                    let _ = self.insert_order(fresh_order);
                }
            }
            // cannot modify side
            None => return Ok(()),
        }

        Ok(())
    }

    pub fn cancel_order(
        &mut self,
        _cancel_request_type: CancelRequestType,
        order_id: Uuid,
    ) -> Option<Order> {
        if let Some(order) = self.orders.remove(&order_id) {
            let price = order.price;
            let cancelled = match order.side {
                OrderSide::Buy => self.bid_levels.remove_order(&price, &order_id),
                OrderSide::Sell => self.ask_levels.remove_order(&price, &order_id),
            };

            if cancelled {
                return Some(order);
            }
        }

        None
    }

    pub fn insert_order(&mut self, order: Order) -> Result<Vec<Trade>> {
        ORDER_COUNTER.inc();

        if self.orders.contains_key(&order.id) {
            return Err(anyhow!("Order id already in use"));
        }

        let _ = self.orders.insert(order.id, order);

        match &order.side {
            OrderSide::Buy => self.bid_levels.insert_order(order.price, order.id),
            OrderSide::Sell => self.ask_levels.insert_order(order.price, order.id),
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
                let _ = self.cancel_order(CancelRequestType::Internal, *order_id);
            }
            OrderType::Normal => {}
        }

        Ok(())
    }

    fn can_match(&mut self, side: &OrderSide, price: &Price) -> bool {
        match side {
            OrderSide::Buy => self
                .ask_levels
                .get_best_price()
                .map_or(false, |best_price| price >= best_price),
            OrderSide::Sell => self
                .bid_levels
                .get_best_price()
                .map_or(false, |best_price| price <= best_price),
        }
    }

    // fn process_trade(bid: &mut Order, ask: &mut Order) -> Result<Trade, ProcessTradeError> {
    //     if ask.price != bid.price {
    //         return Err(ProcessTradeError::PriceDiscrepancy);
    //     }

    //     let quantity = min(ask.remaining_quantity, bid.remaining_quantity);

    //     if quantity < ask.minimum_quantity || quantity < bid.minimum_quantity {
    //         let mut quantity_errors = vec![];
    //         if quantity < ask.minimum_quantity {
    //             quantity_errors.push(MinQuantityNotMetTypes::Ask);
    //         }
    //         if quantity < bid.minimum_quantity {
    //             quantity_errors.push(MinQuantityNotMetTypes::Bid);
    //         }
    //         return Err(ProcessTradeError::MinQuantityNotMet(quantity_errors));
    //     }

    //     bid.fill(quantity)?;
    //     ask.fill(quantity)?;

    //     let trade = Trade {
    //         bid: (*bid, quantity).into(),
    //         ask: (*ask, quantity).into(),
    //     };

    //     TRADE_COUNTER.inc();

    //     Ok(trade)
    // }

    fn calc_trade(bid: &Order, ask: &Order) -> Result<Quantity, ProcessTradeError> {
        if ask.price != bid.price {
            return Err(ProcessTradeError::PriceDiscrepancy);
        }

        let quantity = min(ask.remaining_quantity, bid.remaining_quantity);

        if quantity < ask.minimum_quantity || quantity < bid.minimum_quantity {
            let mut quantity_errors = vec![];
            if quantity < ask.minimum_quantity {
                quantity_errors.push(MinQuantityNotMetTypes::Ask);
            }
            if quantity < bid.minimum_quantity {
                quantity_errors.push(MinQuantityNotMetTypes::Bid);
            }
            return Err(ProcessTradeError::MinQuantityNotMet(quantity_errors));
        }

        Ok(quantity)
    }

    fn match_orders_new(&mut self) -> Result<Vec<Trade>> {
        let mut trades = vec![];

        let mut bid_prices = self.bid_levels.get_prices().into_iter();
        let mut ask_prices = self.ask_levels.get_prices().into_iter();

        let mut best_bid_price = bid_prices.next();
        let mut best_ask_price = ask_prices.next();

        loop {
            match (best_bid_price, best_ask_price) {
                (Some(best_bid_price), Some(best_ask_price)) => {
                    // if no further matches are possible break
                    if best_ask_price > best_bid_price {
                        break;
                    }

                    // attempt to match order at current price levels
                }
                _ => break,
            }
        }

        Ok(trades)
    }

    // TODO: This should return some error signifying that either bid_prices.next() or ask_prices.next()
    fn match_orders_at_price_levels(
        &mut self,
        best_bid_price: &Price,
        best_ask_price: &Price,
    ) -> Result<Trade> {
        let mut ask_ids = self
            .ask_levels
            .get_orders(best_ask_price)
            .context("Should have orders")?
            .into_iter();

        let mut bid_ids = self
            .bid_levels
            .get_orders(best_bid_price)
            .context("Should have orders")?
            .into_iter();

        let mut bid_id = bid_ids.next().context("")?;
        let mut ask_id = ask_ids.next().context("")?;

        todo!()
    }

    // Todo: clean up order levels after rather than during?
    fn match_orders(&mut self) -> Result<Vec<Trade>> {
        let mut trades = vec![];

        let mut bid_level_offset: usize = 0;
        let mut ask_level_offset: usize = 0;

        let ask_prices = self.ask_levels.get_prices();
        let bid_prices = self.bid_levels.get_prices();

        loop {
            let best_bid_price: &i64 = *bid_prices
                .get(bid_level_offset)
                .context("Should never be out of range")?;
            let best_ask_price: &i64 = *ask_prices
                .get(ask_level_offset)
                .context("Should never be out of range")?;

            // if no further trades can be made, break
            if best_ask_price > best_ask_price {
                break;
            }

            // get iterator to bid & ask orders
            let mut ask_ids = self
                .ask_levels
                .get_orders(best_ask_price)
                .context("Should have orders")?
                .into_iter();

            let mut bid_ids = self
                .bid_levels
                .get_orders(best_bid_price)
                .context("Should have orders")?
                .into_iter();

            let mut bid_id = bid_ids.next().context("")?;
            let mut ask_id = ask_ids.next().context("")?;

            loop {
                let bid = self.orders.get(bid_id).context("")?;
                let ask = self.orders.get(ask_id).context("")?;

                let trade_quantity = Orderbook::calc_trade(bid, ask);

                match trade_quantity {
                    Ok(quantity) => {
                        let bid_trade_info: TradeInfo = (bid, quantity).into();
                        let ask_trade_info: TradeInfo = (ask, quantity).into();

                        let mutable_bid = self.orders.get_mut(bid_id).context("")?;
                        let _ = mutable_bid.fill(quantity);
                        let mutable_ask = self.orders.get_mut(ask_id).context("")?;
                        let _ = mutable_ask.fill(quantity);

                        trades.push(Trade {
                            bid: bid_trade_info,
                            ask: ask_trade_info,
                        });

                        TRADE_COUNTER.inc();
                    }
                    Err(e) => match e {
                        ProcessTradeError::MinQuantityNotMet(errors) => {
                            for error in errors {
                                match error {
                                    MinQuantityNotMetTypes::Ask => match ask_ids.next() {
                                        Some(new_ask_id) => ask_id = new_ask_id,
                                        None => {
                                            ask_level_offset += 1;
                                            break;
                                        }
                                    },
                                    MinQuantityNotMetTypes::Bid => match bid_ids.next() {
                                        Some(new_bid_id) => bid_id = new_bid_id,
                                        None => {
                                            bid_level_offset += 1;
                                            break;
                                        }
                                    },
                                }
                            }
                        }
                        _ => return Err(anyhow!("Process trade error")),
                    },
                }
            }
        }

        Ok(trades)
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     fn assert_empty_orderbook(orderbook: &Orderbook) {
//         assert!(orderbook.bid_levels.is_empty());
//         assert!(orderbook.ask_levels.is_empty())
//     }

//     #[test]
//     fn basic_order_match() {
//         let mut orderbook = Orderbook::new();
//         let price = 10;
//         let quantity = 1;

//         let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, quantity, 0).unwrap();
//         let sell_order =
//             Order::new(OrderType::Normal, OrderSide::Sell, price, quantity, 0).unwrap();

//         let first_trades = orderbook.add_order(buy_order).unwrap();
//         let second_trades = orderbook.add_order(sell_order).unwrap();

//         assert!(first_trades.is_empty());

//         let trade = second_trades.first().unwrap();
//         assert_eq!(trade.ask.price, price);
//         assert_eq!(trade.ask.quantity, quantity);

//         assert_eq!(trade.bid.price, price);
//         assert_eq!(trade.bid.quantity, quantity);

//         assert_empty_orderbook(&orderbook)
//     }

//     #[test]
//     fn partial_order_match() {
//         let mut orderbook = Orderbook::new();
//         let price = 10;
//         let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, 5, 0).unwrap();
//         let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, price, 10, 0).unwrap();

//         let first_trades = orderbook.add_order(buy_order).unwrap();
//         let second_trades = orderbook.add_order(sell_order).unwrap();

//         assert!(first_trades.is_empty());

//         let trade = second_trades.first().unwrap();
//         assert_eq!(trade.ask.price, price);
//         assert_eq!(trade.ask.quantity, 5);

//         assert_eq!(trade.bid.price, price);
//         assert_eq!(trade.bid.quantity, 5);

//         assert!(orderbook.bid_levels.is_empty());
//         assert!(!orderbook.ask_levels.is_empty());
//     }

//     #[test]
//     fn fill_or_kill_order() {
//         let mut orderbook = Orderbook::new();
//         let order = Order::new(OrderType::Kill, OrderSide::Buy, 1, 1, 0).unwrap();

//         let trades = orderbook.add_order(order).unwrap();

//         assert!(trades.is_empty());

//         assert_empty_orderbook(&orderbook)
//     }
// }
