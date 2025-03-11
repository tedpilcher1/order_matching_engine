use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use crossbeam::channel::{Receiver, Sender};
use priority_queue::PriorityQueue;
use uuid::Uuid;

use crate::web_server::{CancelRequestType, OrderRequest};

use super::{OrderExpirationRequest, UnixTimestamp};

pub struct ExpirationHandler {
    cancellation_request_sender: Sender<OrderRequest>,
    expiration_order_request_reciever: Receiver<OrderExpirationRequest>,
    expiration_queue: PriorityQueue<Uuid, UnixTimestamp>,
}
}
