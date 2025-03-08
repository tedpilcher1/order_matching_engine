use std::sync::Mutex;

use actix_web::{get, post, web, App, HttpResponse, HttpServer, Responder};
use matching_engine::{
    metrics::{register_custom_metrics, REGISTRY},
    orderbook::types::{Order, Orderbook},
    web_server::types::OrderRequest,
};
use prometheus::{Encoder, TextEncoder};

struct OrderbookMutex {
    orderbook: Mutex<Orderbook>,
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

#[get("/metrics")]
async fn metrics_endpoint() -> impl Responder {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer).unwrap();

    HttpResponse::Ok()
        .content_type("text/plain; version=0.0.4")
        .body(buffer)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    register_custom_metrics();

    let orderbook = OrderbookMutex {
        orderbook: Mutex::new(Orderbook::new()),
    };

    let app_data = web::Data::new(orderbook);

    HttpServer::new(move || {
        App::new()
            .app_data(app_data.clone())
            .service(metrics_endpoint)
            .service(create_order_endpoint)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
