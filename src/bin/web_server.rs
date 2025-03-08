use std::thread;

use actix_web::{web, App, HttpServer};
use crossbeam::channel::{self, Receiver};
use matching_engine::{
    metrics::register_custom_metrics,
    orderbook::orderbook::Orderbook,
    web_server::{
        endpoints::{
            cancel_order_endpoint, create_order_endpoint, metrics_endpoint, modify_order_endpoint,
        },
        types::{AppState, OrderRequest},
    },
};

fn worker_thread(receiver: Receiver<OrderRequest>) {
    let mut orderbook = Orderbook::new();

    loop {
        if let Ok(order_request) = receiver.recv() {
            match order_request {
                OrderRequest::Trade(trade_request) => {
                    let _ = orderbook.add_order(trade_request.into());
                }
                OrderRequest::Cancel(order_id) => {
                    let _ = orderbook.cancel_order(order_id);
                }
                OrderRequest::Modify(trade_request) => {
                    let _ = orderbook.modify_order(trade_request.into());
                }
            }
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    register_custom_metrics();

    let (sender, receiver) = channel::unbounded();

    thread::spawn(move || {
        worker_thread(receiver);
    });

    let state = web::Data::new(AppState { sender });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(metrics_endpoint)
            .service(create_order_endpoint)
            .service(cancel_order_endpoint)
            .service(modify_order_endpoint)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
