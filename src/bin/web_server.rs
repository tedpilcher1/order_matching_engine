use std::thread;

use actix_web::{web, App, HttpServer};
use crossbeam::channel;
use matching_engine::{
    metrics::register_custom_metrics,
    orderbook::orderbook::{Order, Orderbook},
    web_server::{
        endpoints::{create_order_endpoint, metrics_endpoint},
        types::AppState,
    },
};

fn worker_thread(receiver: crossbeam::channel::Receiver<Order>) {
    let mut orderbook = Orderbook::new();

    loop {
        if let Ok(order) = receiver.recv() {
            let _ = orderbook.add_order(order);
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
        // .service(cancel_order_endpoint)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
