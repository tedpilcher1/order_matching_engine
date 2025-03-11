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

impl ExpirationHandler {
    pub fn new(
        cancellation_request_sender: Sender<OrderRequest>,
        expiration_order_request_reciever: Receiver<OrderExpirationRequest>,
    ) -> Self {
        Self {
            cancellation_request_sender,
            expiration_order_request_reciever,
            expiration_queue: PriorityQueue::new(),
        }
    }
    fn send_cancellation_request(&mut self, order_id: Uuid) -> Result<()> {
        let order_request = OrderRequest::Cancel(CancelRequestType::Internal, order_id);

        match self.cancellation_request_sender.send(order_request) {
            Ok(_) => Ok(()),
            Err(_) => Err(anyhow!(
                "Failed to send cancellation request order to orderbook"
            )),
        }
    }
}
