use uuid::Uuid;

pub mod expiration_handler;

type UnixTimestamp = i64;

pub enum ExpirationOrderRequest {
    InsertExpirationRequest(InsertExpirationRequest),
    RemoveExpirationRequest(Uuid),
}

pub struct InsertExpirationRequest {
    timestamp: UnixTimestamp,
    order_id: Uuid,
}
