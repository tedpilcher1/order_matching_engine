use uuid::Uuid;

pub mod expiration_handler;

type UnixTimestamp = i64;

pub enum ExpirationOrderRequest {
    InsertExpirationRequest(InsertExpirationRequest),
    RemoveExpirationRequest(Uuid),
}

pub struct InsertExpirationRequest {
    pub timestamp: UnixTimestamp,
    pub order_id: Uuid,
}
