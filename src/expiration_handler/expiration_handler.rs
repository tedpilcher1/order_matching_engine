use std::cmp::Reverse;

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
    expiration_queue: PriorityQueue<Uuid, Reverse<UnixTimestamp>>,
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

    pub fn run(&mut self) {
        loop {
            if let Ok(order_expiration_request) = self.expiration_order_request_reciever.try_recv()
            {
                let _ = self.insert_expiring_order(order_expiration_request);
            }

            if let Some(order) = self.expiration_queue.peek() {
                if order.1 .0 < Utc::now().timestamp() {
                    // TODO: Need to handle this error, might just be best to log it
                    let _ = self.send_cancellation_request(*order.0);
                    self.expiration_queue.pop();
                }
            }
        }
    }

    fn insert_expiring_order(
        &mut self,
        order_expiration_request: OrderExpirationRequest,
    ) -> Result<()> {
        if order_expiration_request.timestamp < Utc::now().timestamp() {
            bail!("Timestamp in past")
        }

        self.expiration_queue.push(
            order_expiration_request.order_id,
            Reverse(order_expiration_request.timestamp),
        );

        Ok(())
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
