use actix_web::{get, post, web, HttpResponse, Responder};

use prometheus::{Encoder, TextEncoder};

use crate::{
    metrics::{REGISTRY, REQUESTS_COUNTER},
    web_server::types::{AppState, OrderRequest},
};

// #[post("/cancel_order{order_id}")]
// async fn cancel_order_endpoint(
//     order_id: web::Path<Uuid>,
//     orderbook: web::Data<OrderbookMutex>,
// ) -> impl Responder {
//     REQUESTS_COUNTER.inc();
//     let mut orderbook = match orderbook.orderbook.lock() {
//         Ok(orderbook) => orderbook,
//         Err(_) => return HttpResponse::InternalServerError().finish(),
//     };

//     match orderbook.cancel_order(*order_id) {
//         Ok(cancelled) => HttpResponse::Ok().json(cancelled),
//         Err(_) => HttpResponse::InternalServerError().finish(),
//     }
// }

#[post("/create_order")]
async fn create_order_endpoint(
    order_request: web::Json<OrderRequest>,
    state: web::Data<AppState>,
) -> impl Responder {
    REQUESTS_COUNTER.inc();
    match state.sender.send(order_request.into_inner().into()) {
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
