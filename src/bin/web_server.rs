use std::sync::{Arc, Mutex};

use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use matching_engine::orderbook::types::{Order, OrderSide, OrderType, Orderbook};
use serde::Deserialize;
use uuid::Uuid;

type Price = i64;
type Quantity = u64;

struct OrderbookMutex {
    orderbook: Mutex<Orderbook>,
}

#[derive(Deserialize)]
pub struct OrderRequest {
    order_type: OrderType,
    order_side: OrderSide,
    price: Price,
    quantity: Quantity,
}

impl From<OrderRequest> for Order {
    fn from(order_request: OrderRequest) -> Self {
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

#[post("/create_order")]
async fn create_order_endpoint(
    order: web::Json<OrderRequest>,
    orderbook: web::Data<OrderbookMutex>,
) -> impl Responder {
    let mut orderbook = match orderbook.orderbook.lock() {
        Ok(orderbook) => orderbook,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };

    let order: Order = order.into_inner().into();
    match orderbook.add_order(order) {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let orderbook = OrderbookMutex {
        orderbook: Mutex::new(Orderbook::new()),
    };

    let app_data = web::Data::new(orderbook);

    HttpServer::new(move || {
        App::new()
            .app_data(app_data.clone())
            .service(create_order_endpoint)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
