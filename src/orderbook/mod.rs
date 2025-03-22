use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::web_server::CancelRequestType;

pub mod orderbook;
pub mod orderlevels;

type Price = i64;
type Quantity = u64;

#[derive(Copy, Clone, PartialEq, Debug, BorshSerialize, BorshDeserialize)]
pub struct Order {
    pub type_: OrderType,
    pub id: Uuid,
    pub side: OrderSide,
    pub price: Price,
    pub initial_quantity: Quantity,
    pub remaining_quantity: Quantity,
    pub minimum_quantity: Quantity,
    pub virtual_remaining_quantity: Quantity,
}

impl Order {
    pub fn new(
        type_: OrderType,
        side: OrderSide,
        price: Price,
        quantity: Quantity,
        minimum_quantity: Quantity,
    ) -> Self {
        Self {
            type_,
            id: Uuid::new_v4(),
            side,
            price,
            initial_quantity: quantity,
            remaining_quantity: quantity,
            minimum_quantity,
            virtual_remaining_quantity: quantity,
        }
    }
}

#[derive(
    Copy, Clone, PartialEq, Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize,
)]
pub enum OrderType {
    Normal,
    Kill,
}

#[derive(
    PartialEq, Clone, Copy, Debug, Deserialize, Serialize, BorshSerialize, BorshDeserialize,
)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(BorshDeserialize, Debug, PartialEq, BorshSerialize, Clone)]
struct TradeInfo {
    order_id: Uuid,
    price: Price,
    quantity: Quantity,
}

/// matched order, aggregate of bid and ask
#[derive(Debug, PartialEq, BorshSerialize, BorshDeserialize, Clone)]
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

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct CancelledOrder {
    cancel_request_type: CancelRequestType,
    order: Order,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub enum MarketDataUpdate {
    Trade(Trade),
    Cancellation(CancelledOrder),
}
