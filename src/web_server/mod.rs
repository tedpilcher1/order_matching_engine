use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::orderbook::{Order, OrderSide, OrderType};

pub mod endpoints;

type Price = i64;
type Quantity = u64;

#[derive(Deserialize, Serialize)]
pub enum OrderRequest {
    Trade(TradeRequest),
    Cancel(CancelRequestType, Uuid),
    Modify(TradeRequest),
}

#[derive(Deserialize, Serialize)]
pub enum CancelRequestType {
    Internal,
    External,
}

#[derive(Deserialize, Serialize)]
pub struct TradeRequest {
    pub order_type: OrderType,
    pub order_side: OrderSide,
    pub price: Price,
    pub quantity: Quantity,
    pub minimum_quantity: Quantity,
}

impl TryFrom<TradeRequest> for Order {
    type Error = anyhow::Error;

    fn try_from(trade_request: TradeRequest) -> Result<Self, Self::Error> {
        if trade_request.minimum_quantity > trade_request.quantity {
            return Err(anyhow!("Minimum quantity > quantity"));
        }

        Ok(Order {
            type_: trade_request.order_type,
            id: Uuid::new_v4(),
            side: trade_request.order_side,
            price: trade_request.price,
            initial_quantity: trade_request.quantity,
            remaining_quantity: trade_request.quantity,
            minimum_quantity: trade_request.minimum_quantity,
        })
    }
}

pub struct AppState {
    pub sender: crossbeam::channel::Sender<OrderRequest>,
}
