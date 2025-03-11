use uuid::Uuid;

pub mod expiration_handler;

type UnixTimestamp = i64;

pub struct OrderExpirationRequest {
    timestamp: UnixTimestamp,
    order_id: Uuid,
}
