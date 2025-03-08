use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::orderbook::{Order, OrderSide, OrderType};

type Price = i64;
type Quantity = u64;

#[derive(Deserialize, Serialize)]
pub enum OrderRequest {
    Trade(TradeRequest),
    Cancel(Uuid),
}

#[derive(Deserialize, Serialize)]
pub struct TradeRequest {
    pub order_type: OrderType,
    pub order_side: OrderSide,
    pub price: Price,
    pub quantity: Quantity,
}

impl From<TradeRequest> for Order {
    fn from(order_request: TradeRequest) -> Self {
        Order {
            type_: OrderType::Normal,
            id: Uuid::new_v4(),
            side: order_request.order_side,
            price: order_request.price,
            initial_quantity: order_request.quantity,
            remaining_quantity: order_request.quantity,
        }
    }
}

pub struct AppState {
    pub sender: crossbeam::channel::Sender<OrderRequest>,
}
