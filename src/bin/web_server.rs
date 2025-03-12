use std::thread;

use actix_web::{web, App, HttpServer};
use crossbeam::channel::{self, Receiver};
use order_matching_engine::{
    expiration_handler::expiration_handler::ExpirationHandler,
    metrics::register_custom_metrics,
    orderbook::{orderbook::Orderbook, Order},
    web_server::{
        endpoints::{
            cancel_order_endpoint, cancel_order_expiration_endpoint, create_order_endpoint,
            metrics_endpoint, modify_order_endpoint,
        },
        AppState, OrderRequest,
    },
};

fn worker_thread(receiver: Receiver<OrderRequest>) {
    let mut orderbook = Orderbook::new();

    loop {
        if let Ok(order_request) = receiver.recv() {
            match order_request {
                OrderRequest::Trade(trade_request) => {
                    if let Ok(order_request) = Order::try_from(trade_request) {
                        let _ = orderbook.insert_order(order_request);
                    }
                }
                OrderRequest::Cancel(cancel_request_type, order_id) => {
                    let _ = orderbook.cancel_order(cancel_request_type, order_id);
                }
                OrderRequest::Modify(trade_request) => {
                    if let Ok(order_request) = Order::try_from(trade_request) {
                        let _ = orderbook.modify_order(order_request);
                    }
                }
            }
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    register_custom_metrics();

    let (order_engine_sender, order_engine_receiver) = channel::unbounded();
    let (order_expiration_sender, order_expiration_receiver) = channel::unbounded();
    let cancellation_request_sender = order_engine_sender.clone();

    thread::spawn(move || {
        let mut expiration_handler =
            ExpirationHandler::new(cancellation_request_sender, order_expiration_receiver);
        expiration_handler.run();
    });

    thread::spawn(move || {
        worker_thread(order_engine_receiver);
    });

    let state = web::Data::new(AppState {
        order_engine_sender,
        order_expiration_sender,
    });

    HttpServer::new(move || {
        App::new()
            .app_data(state.clone())
            .service(metrics_endpoint)
            .service(create_order_endpoint)
            .service(cancel_order_endpoint)
            .service(modify_order_endpoint)
            .service(cancel_order_expiration_endpoint)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
