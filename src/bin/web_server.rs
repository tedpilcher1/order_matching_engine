use std::thread;

use actix_web::{web, App, HttpServer};
use crossbeam::channel::{self, Receiver, Sender};
use order_matching_engine::{
    expiration_handler::expiration_handler::ExpirationHandler,
    market_data_outbox::market_data_outbox_worker::MarketDataWorker,
    metrics::register_custom_metrics,
    orderbook::{orderbook::Orderbook, MarketDataUpdate},
    web_server::{
        endpoints::{
            cancel_order_endpoint, cancel_order_expiration_endpoint, create_order_endpoint,
            metrics_endpoint, modify_order_endpoint,
        },
        AppState, OrderRequest,
    },
};

fn worker_thread(receiver: Receiver<OrderRequest>, market_data_sender: Sender<MarketDataUpdate>) {
    let mut orderbook = Orderbook::new(Some(market_data_sender));

    loop {
        if let Ok(order_request) = receiver.recv() {
            let _ = orderbook.place_trade_request(order_request);
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    register_custom_metrics();

    let (order_engine_sender, order_engine_receiver) = channel::unbounded();
    let (order_expiration_sender, order_expiration_receiver) = channel::unbounded();
    let (market_data_sender, market_data_reciever) = channel::unbounded();
    let cancellation_request_sender = order_engine_sender.clone();

    thread::spawn(async move || {
        let mut market_data_worker = MarketDataWorker::new(market_data_reciever);
        market_data_worker.do_work().await;
    });

    thread::spawn(move || {
        let mut expiration_handler =
            ExpirationHandler::new(cancellation_request_sender, order_expiration_receiver);
        expiration_handler.run();
    });

    thread::spawn(move || {
        worker_thread(order_engine_receiver, market_data_sender);
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
