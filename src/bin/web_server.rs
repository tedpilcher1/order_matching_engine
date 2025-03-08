use std::sync::Mutex;

use actix_web::{web, App, HttpServer};
use matching_engine::{
    metrics::register_custom_metrics,
    orderbook::orderbook::Orderbook,
    web_server::{
        endpoints::{cancel_order_endpoint, create_order_endpoint, metrics_endpoint},
        types::OrderbookMutex,
    },
};

#[tokio::main]
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
            .service(cancel_order_endpoint)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
