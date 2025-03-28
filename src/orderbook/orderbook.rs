use std::{cmp::min, collections::HashMap};

use anyhow::{bail, Result};
use chrono::Utc;
use crossbeam::channel::Sender;
use uuid::Uuid;

use crate::{
    metrics::{MATCHING_DURATION, ORDERS_FILLED_COUNTER, ORDER_COUNTER, TRADE_COUNTER},
    orderbook::CancelledOrder,
    web_server::{CancelRequestType, OrderRequest},
};

use super::{
    orderlevels::{AskOrderLevels, BidOrderLevels, OrderLevels},
    MarketDataUpdate, Order, OrderSide, OrderType, Trade, TradeInfo,
};

#[derive(Debug)]
pub struct Orderbook {
    ask_levels: AskOrderLevels,
    bid_levels: BidOrderLevels,
    orders: HashMap<Uuid, Order>,
    market_data_update_sender: Option<Sender<MarketDataUpdate>>,
}

impl Default for Orderbook {
    fn default() -> Self {
        Self::new(None)
    }
}

impl Orderbook {
    pub fn new(market_data_update_sender: Option<Sender<MarketDataUpdate>>) -> Self {
        Self {
            ask_levels: AskOrderLevels::new(),
            bid_levels: BidOrderLevels::new(),
            orders: HashMap::new(),
            market_data_update_sender,
        }
    }

    /// Matches and handles trade request
    ///
    /// Only pub access to orderbook
    ///
    /// Sends series of market updates to
    /// market update worker to expose
    pub fn place_trade_request(
        &mut self,
        order_request: OrderRequest,
    ) -> Result<Vec<MarketDataUpdate>> {
        let market_updates: Vec<MarketDataUpdate> = match order_request {
            OrderRequest::Trade(trade_request) => match trade_request.try_into() {
                Ok(order) => match self.match_order(order) {
                    Ok(trades) => trades.into_iter().map(MarketDataUpdate::Trade).collect(),
                    Err(_) => vec![],
                },
                Err(_) => vec![],
            },
            OrderRequest::Cancel(cancel_request_type, order_id) => {
                match self.cancel_order(cancel_request_type, order_id) {
                    Some(cancelled_order) => vec![MarketDataUpdate::Cancellation(cancelled_order)],
                    None => vec![],
                }
            }
            OrderRequest::Modify(trade_request) => match trade_request.try_into() {
                Ok(order) => match self.modify_order(order) {
                    Ok((cancelled_order, trades)) => {
                        let mut updates = vec![MarketDataUpdate::Cancellation(cancelled_order)];
                        updates.extend(trades.into_iter().map(MarketDataUpdate::Trade));
                        updates
                    }
                    Err(_) => vec![],
                },
                Err(_) => vec![],
            },
        };

        if let Some(sender) = &self.market_data_update_sender {
            for market_data_update in &market_updates {
                let _ = sender.send(market_data_update.clone());
            }
        }

        Ok(market_updates)
    }

    fn match_order(&mut self, mut order: Order) -> Result<Vec<Trade>> {
        ORDER_COUNTER.inc();

        if self.orders.contains_key(&order.id) {
            bail!("Order id already in use")
        }

        let trades = match self.can_match_order(&order) {
            true => {
                let start_time = Utc::now().timestamp();
                let trades = self.internal_match_order(&mut order);
                let end_time = Utc::now().timestamp();
                MATCHING_DURATION.observe((end_time - start_time) as f64);
                trades
            }
            false => vec![],
        };

        if order.type_ == OrderType::Normal && order.remaining_quantity > 0 {
            self.insert_order(order)
        }

        if order.remaining_quantity == 0 {
            ORDERS_FILLED_COUNTER.inc();
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
            if order.remaining_quantity == 0 {
                break;
            }

            let opposing_orders = match order.side {
                OrderSide::Buy => self.ask_levels.get_orders(price_level),
                OrderSide::Sell => self.bid_levels.get_orders(price_level),
            };

            if let Some(opposing_orders) = opposing_orders {
                for opposing_order_id in opposing_orders {
                    if order.virtual_remaining_quantity == 0 {
                        break;
                    }

                    let opposing_order = self
                        .orders
                        .get_mut(opposing_order_id)
                        .expect("Order should never be in price level but not in orders");

                    let quantity = min(
                        order.virtual_remaining_quantity,
                        opposing_order.virtual_remaining_quantity,
                    );

                    if quantity < opposing_order.minimum_quantity {
                        continue;
                    }

                    order.virtual_remaining_quantity -= quantity;
                    opposing_order.virtual_remaining_quantity -= quantity;

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

        if (order.initial_quantity - order.virtual_remaining_quantity) >= order.minimum_quantity {
            self.commit_trades(order, &trades);
            trades
        } else {
            self.discard_trades(order, &trades);
            vec![]
        }
    }

    fn discard_trades(&mut self, order: &mut Order, trades: &Vec<Trade>) {
        for trade in trades {
            let opposing_order_id = match order.side {
                OrderSide::Buy => trade.ask.order_id,
                OrderSide::Sell => trade.bid.order_id,
            };

            let opposing_order = self
                .orders
                .get_mut(&opposing_order_id)
                .expect("Order shouldn't have been removed yet");

            opposing_order.virtual_remaining_quantity = opposing_order.remaining_quantity
        }
        order.virtual_remaining_quantity = order.remaining_quantity
    }

    fn commit_trades(&mut self, order: &mut Order, trades: &Vec<Trade>) {
        for trade in trades {
            let opposing_order_id = match order.side {
                OrderSide::Buy => trade.ask.order_id,
                OrderSide::Sell => trade.bid.order_id,
            };

            let opposing_order = self
                .orders
                .get_mut(&opposing_order_id)
                .expect("Order shouldn't have been removed yet");

            opposing_order.remaining_quantity = opposing_order.virtual_remaining_quantity;

            if opposing_order.remaining_quantity == 0 {
                ORDERS_FILLED_COUNTER.inc();
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
            TRADE_COUNTER.inc();
        }

        order.remaining_quantity = order.virtual_remaining_quantity;
        self.ask_levels.remove_empty_levels();
        self.bid_levels.remove_empty_levels();
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
    fn modify_order(&mut self, order: Order) -> Result<(CancelledOrder, Vec<Trade>)> {
        let existing_order = match self.orders.get(&order.id) {
            Some(existing) => existing,
            None => bail!("Order not found"),
        };

        if existing_order.type_ != order.type_ {
            bail!("Cannot modify order type")
        }

        if (existing_order.initial_quantity - existing_order.remaining_quantity)
            > order.initial_quantity
        {
            bail!("Cannot modify quantity to lower than currently filled")
        }

        let cancelled_order = self
            .cancel_order(CancelRequestType::Internal, order.id)
            .ok_or_else(|| anyhow::anyhow!("Could not cancel order"))?;

        let fresh_order = Order {
            type_: order.type_,
            id: order.id,
            side: order.side,
            price: order.price,
            initial_quantity: order.initial_quantity,
            remaining_quantity: cancelled_order.order.remaining_quantity,
            virtual_remaining_quantity: cancelled_order.order.remaining_quantity,
            minimum_quantity: cancelled_order.order.minimum_quantity,
        };
        let trades = self.match_order(fresh_order).unwrap_or_default();
        Ok((cancelled_order, trades))
    }

    fn cancel_order(
        &mut self,
        cancel_request_type: CancelRequestType,
        order_id: Uuid,
    ) -> Option<CancelledOrder> {
        if let Some(order) = self.orders.remove(&order_id) {
            let price = order.price;
            let cancelled = match order.side {
                OrderSide::Buy => self.bid_levels.remove_order(&price, &order_id),
                OrderSide::Sell => self.ask_levels.remove_order(&price, &order_id),
            };

            if cancelled {
                return Some(CancelledOrder {
                    cancel_request_type,
                    order,
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use crate::orderbook::{Price, Quantity};

    use super::*;

    fn assert_trade(trades: &Vec<Trade>, index: usize, bid: TradeInfo, ask: TradeInfo) {
        let trade = trades.get(index).unwrap();
        assert_eq!(trade.bid, bid);
        assert_eq!(trade.ask, ask)
    }

    fn assert_empty_book(orderbook: &Orderbook) {
        assert!(orderbook.orders.is_empty());
        assert!(orderbook.ask_levels.get_prices().is_empty());
        assert!(orderbook.bid_levels.get_prices().is_empty());
    }

    fn assert_book_has_order(
        orderbook: &Orderbook,
        order_id: &Uuid,
        order_side: &OrderSide,
        remaining_quantity: &Quantity,
        price: &Price,
    ) {
        let order = orderbook.orders.get(order_id).unwrap();
        assert_eq!(order.remaining_quantity, *remaining_quantity);
        match order_side {
            OrderSide::Buy => assert!(orderbook
                .bid_levels
                .get_orders(price)
                .unwrap()
                .contains(order_id)),

            OrderSide::Sell => assert!(orderbook
                .ask_levels
                .get_orders(price)
                .unwrap()
                .contains(order_id)),
        }
    }

    fn assert_empty_asks(orderbook: &Orderbook) {
        assert!(orderbook.ask_levels.get_prices().is_empty())
    }

    fn assert_empty_bids(orderbook: &Orderbook) {
        assert!(orderbook.bid_levels.get_prices().is_empty())
    }

    #[test]
    fn can_insert_order() {
        let mut orderbook = Orderbook::default();
        let price = 1;
        let quantity = 1;

        let order = Order::new(OrderType::Normal, OrderSide::Buy, price, quantity, 0);
        let trades = orderbook.match_order(order).unwrap();

        assert_eq!(trades.len(), 0);
        assert_book_has_order(&orderbook, &order.id, &order.side, &quantity, &price);
        assert_empty_asks(&orderbook);
    }

    #[test]
    fn cannot_match_orders_when_ask_exceeds_bid() {
        let mut orderbook = Orderbook::default();

        let quantity = 1;
        let bid_price = 1;
        let ask_price = 2;

        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, bid_price, quantity, 0);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, ask_price, quantity, 0);

        let first_trades = orderbook.match_order(buy_order).unwrap();
        let second_trades = orderbook.match_order(sell_order).unwrap();

        assert!(first_trades.is_empty());
        assert!(second_trades.is_empty());

        assert_book_has_order(
            &orderbook,
            &buy_order.id,
            &buy_order.side,
            &quantity,
            &bid_price,
        );

        assert_book_has_order(
            &orderbook,
            &sell_order.id,
            &sell_order.side,
            &quantity,
            &ask_price,
        );
    }

    #[test]
    fn can_kill_order() {
        let mut orderbook = Orderbook::default();
        let price = 1;
        let quantity = 1;

        let order = Order::new(OrderType::Kill, OrderSide::Buy, price, quantity, 0);
        let trades = orderbook.match_order(order).unwrap();

        assert!(trades.is_empty());
        assert_empty_book(&orderbook);
    }

    #[test]
    fn can_match_symmetric_opposing_orders() {
        let mut orderbook = Orderbook::default();
        let price = 1;
        let quantity = 1;

        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, quantity, 0);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, price, quantity, 0);

        let first_trades = orderbook.match_order(buy_order).unwrap();
        let second_trades = orderbook.match_order(sell_order).unwrap();

        assert!(first_trades.is_empty());
        assert_eq!(
            second_trades.first().unwrap(),
            &Trade {
                bid: TradeInfo {
                    order_id: buy_order.id,
                    price,
                    quantity,
                },
                ask: TradeInfo {
                    order_id: sell_order.id,
                    price,
                    quantity,
                }
            }
        );
        assert_empty_book(&orderbook);
    }

    #[test]
    fn can_partially_fill_orders() {
        let mut orderbook = Orderbook::default();
        let price = 1;

        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, 1, 0);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, price, 2, 0);

        let first_trades = orderbook.match_order(buy_order).unwrap();
        let second_trades = orderbook.match_order(sell_order).unwrap();

        assert!(first_trades.is_empty());
        assert_trade(
            &second_trades,
            0,
            TradeInfo {
                order_id: buy_order.id,
                price,
                quantity: 1,
            },
            TradeInfo {
                order_id: sell_order.id,
                price,
                quantity: 1,
            },
        );
        assert_empty_bids(&orderbook);
        assert_book_has_order(&orderbook, &sell_order.id, &sell_order.side, &1, &price);
    }

    #[test]
    fn can_match_orders_with_different_prices() {
        let mut orderbook = Orderbook::default();
        let quantity = 1;
        let buy_price = 2;
        let sell_price = 1;

        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, buy_price, quantity, 0);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, sell_price, quantity, 0);

        let first_trades = orderbook.match_order(buy_order).unwrap();
        let second_trades = orderbook.match_order(sell_order).unwrap();

        assert!(first_trades.is_empty());
        assert_trade(
            &second_trades,
            0,
            TradeInfo {
                order_id: buy_order.id,
                price: buy_price,
                quantity,
            },
            TradeInfo {
                order_id: sell_order.id,
                price: sell_price,
                quantity,
            },
        );
        assert_empty_book(&orderbook);
    }

    #[test]
    fn can_fill_with_multiple_opposing_orders() {
        let mut orderbook = Orderbook::default();
        let price = 1;

        let buy_order_1 = Order::new(OrderType::Normal, OrderSide::Buy, price, 1, 0);
        let buy_order_2 = Order::new(OrderType::Normal, OrderSide::Buy, price, 2, 0);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, price, 3, 0);

        let first_trades = orderbook.match_order(buy_order_1).unwrap();
        let second_trades = orderbook.match_order(buy_order_2).unwrap();
        let third_trades = orderbook.match_order(sell_order).unwrap();

        assert!(first_trades.is_empty());
        assert!(second_trades.is_empty());
        assert_trade(
            &third_trades,
            0,
            TradeInfo {
                order_id: buy_order_1.id,
                price,
                quantity: 1,
            },
            TradeInfo {
                order_id: sell_order.id,
                price,
                quantity: 1,
            },
        );
        assert_trade(
            &third_trades,
            1,
            TradeInfo {
                order_id: buy_order_2.id,
                price,
                quantity: 2,
            },
            TradeInfo {
                order_id: sell_order.id,
                price,
                quantity: 2,
            },
        );
        assert_empty_book(&orderbook);
    }

    #[test]
    fn order_not_filled_when_min_quantity_not_met() {
        let mut orderbook = Orderbook::default();
        let price = 1;

        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, 1, 0);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, price, 2, 2);

        let first_trades = orderbook.match_order(buy_order).unwrap();
        let second_trades = orderbook.match_order(sell_order).unwrap();

        assert!(first_trades.is_empty());
        assert!(second_trades.is_empty());
        assert_book_has_order(&orderbook, &buy_order.id, &buy_order.side, &1, &price);
        assert_book_has_order(&orderbook, &sell_order.id, &sell_order.side, &2, &price);
    }

    #[test]
    fn order_filled_when_min_quantity_met() {
        let mut orderbook = Orderbook::default();
        let price = 1;
        let quantity = 2;

        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, price, quantity, 0);
        let sell_order = Order::new(
            OrderType::Normal,
            OrderSide::Sell,
            price,
            quantity,
            quantity,
        );

        let first_trades = orderbook.match_order(buy_order).unwrap();
        let second_trades = orderbook.match_order(sell_order).unwrap();
        assert!(first_trades.is_empty());
        assert_trade(
            &second_trades,
            0,
            TradeInfo {
                order_id: buy_order.id,
                price,
                quantity,
            },
            TradeInfo {
                order_id: sell_order.id,
                price,
                quantity,
            },
        );
        assert_empty_book(&orderbook)
    }

    #[test]
    fn resting_order_not_filled_when_min_quantity_not_met() {
        let mut orderbook = Orderbook::default();
        let price = 1;

        let buy_order_1 = Order::new(OrderType::Normal, OrderSide::Buy, price, 1, 5);
        let buy_order_2 = Order::new(OrderType::Normal, OrderSide::Buy, price, 1, 0);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, price, 1, 0);

        let first_trades = orderbook.match_order(buy_order_1).unwrap();
        let second_trades = orderbook.match_order(buy_order_2).unwrap();
        let third_trades = orderbook.match_order(sell_order).unwrap();

        assert!(first_trades.is_empty());
        assert!(second_trades.is_empty());
        assert_trade(
            &third_trades,
            0,
            TradeInfo {
                order_id: buy_order_2.id,
                price,
                quantity: 1,
            },
            TradeInfo {
                order_id: sell_order.id,
                price,
                quantity: 1,
            },
        );
        assert_book_has_order(
            &orderbook,
            &buy_order_1.id,
            &buy_order_1.side,
            &buy_order_1.remaining_quantity,
            &price,
        );
        assert_empty_asks(&orderbook);
    }

    #[test]
    fn can_cancel_order() {
        let mut orderbook = Orderbook::new(None);

        let order = Order::new(OrderType::Normal, OrderSide::Buy, 1, 1, 0);
        let trades = orderbook.match_order(order).unwrap();
        let cancellation = orderbook
            .cancel_order(CancelRequestType::External, order.id)
            .unwrap();

        assert!(trades.is_empty());
        assert_eq!(cancellation.order, order);
        assert_empty_book(&orderbook)
    }

    #[test]
    fn can_modify_order() {
        let mut orderbook = Orderbook::new(None);

        let order = Order::new(OrderType::Normal, OrderSide::Buy, 1, 1, 0);
        let first_trades = orderbook.match_order(order).unwrap();

        let modified_order = Order {
            type_: order.type_,
            id: order.id,
            side: order.side,
            price: 2,
            initial_quantity: 1,
            remaining_quantity: 1,
            minimum_quantity: 1,
            virtual_remaining_quantity: 1,
        };

        let (cancelled_order, second_trades) = orderbook.modify_order(modified_order).unwrap();

        assert!(first_trades.is_empty());
        assert!(second_trades.is_empty());
        assert_eq!(order, cancelled_order.order);
        assert_book_has_order(&orderbook, &modified_order.id, &modified_order.side, &1, &2)
    }

    #[test]
    fn modified_order_can_be_filled() {
        let mut orderbook = Orderbook::new(None);
        let buy_order = Order::new(OrderType::Normal, OrderSide::Buy, 1, 1, 0);
        let sell_order = Order::new(OrderType::Normal, OrderSide::Sell, 2, 1, 0);

        let first_trades = orderbook.match_order(buy_order).unwrap();
        let second_trades = orderbook.match_order(sell_order).unwrap();

        let modified_order = Order {
            type_: sell_order.type_,
            id: sell_order.id,
            side: sell_order.side,
            price: 1,
            initial_quantity: 1,
            remaining_quantity: 1,
            minimum_quantity: 1,
            virtual_remaining_quantity: 1,
        };
        let (cancelled_order, third_trades) = orderbook.modify_order(modified_order).unwrap();

        assert!(first_trades.is_empty());
        assert!(second_trades.is_empty());
        assert_eq!(sell_order, cancelled_order.order);
        assert_trade(
            &third_trades,
            0,
            TradeInfo {
                order_id: buy_order.id,
                price: 1,
                quantity: 1,
            },
            TradeInfo {
                order_id: sell_order.id,
                price: 1,
                quantity: 1,
            },
        );
        assert_empty_book(&orderbook)
    }
}
