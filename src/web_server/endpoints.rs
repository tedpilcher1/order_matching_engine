use actix_web::{get, post, web, HttpResponse, Responder};

use prometheus::{Encoder, TextEncoder};
use uuid::Uuid;

use crate::{
    expiration_handler::{ExpirationOrderRequest, InsertExpirationRequest},
    metrics::{REGISTRY, REQUESTS_COUNTER},
    web_server::{AppState, OrderRequest, TradeRequest},
};

#[post("/modify_order")]
async fn modify_order_endpoint(
    order_request: web::Json<TradeRequest>,
    state: web::Data<AppState>,
) -> impl Responder {
    REQUESTS_COUNTER.inc();
    match state
        .order_engine_sender
        .send(OrderRequest::Modify(order_request.into_inner().into()))
    {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[post("/cancel_order/{order_id}")]
async fn cancel_order_endpoint(
    order_id: web::Path<Uuid>,
    state: web::Data<AppState>,
) -> impl Responder {
    REQUESTS_COUNTER.inc();

    match state.order_engine_sender.send(OrderRequest::Cancel(
        crate::web_server::CancelRequestType::External,
        order_id.into_inner(),
    )) {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}

#[post("/create_order")]
async fn create_order_endpoint(
    order_request: web::Json<TradeRequest>,
    state: web::Data<AppState>,
) -> impl Responder {
    REQUESTS_COUNTER.inc();

    let trade_request = order_request.into_inner();
    let trade_request_id = trade_request.id;
    let expiration_date = trade_request.expiration_date;

    if state
        .order_engine_sender
        .send(OrderRequest::Trade(trade_request.into()))
        .is_err()
    {
        return HttpResponse::InternalServerError().finish();
    }

    if let Some(expiration_date) = expiration_date {
        let expiration_request = InsertExpirationRequest {
            timestamp: expiration_date.and_utc().timestamp(),
            order_id: trade_request_id,
        };

        if state
            .order_expiration_sender
            .send(ExpirationOrderRequest::InsertExpirationRequest(
                expiration_request,
            ))
            .is_err()
        {
            return HttpResponse::InternalServerError().finish();
        }
    }

    HttpResponse::Ok().finish()
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

#[post("/cancel_order_expiration/{order_id}")]
async fn cancel_order_expiration_endpoint(
    order_id: web::Path<Uuid>,
    state: web::Data<AppState>,
) -> impl Responder {
    match state
        .order_expiration_sender
        .send(ExpirationOrderRequest::RemoveExpirationRequest(
            order_id.into_inner(),
        )) {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}
