use std::cmp::Reverse;

use anyhow::{anyhow, bail, Result};
use chrono::Utc;
use crossbeam::channel::{Receiver, Sender};
use priority_queue::PriorityQueue;
use uuid::Uuid;

use crate::web_server::{CancelRequestType, OrderRequest};

use super::{ExpirationOrderRequest, InsertExpirationRequest, UnixTimestamp};

pub struct ExpirationHandler {
    cancellation_request_sender: Sender<OrderRequest>,
    expiration_order_request_reciever: Receiver<ExpirationOrderRequest>,
    expiration_queue: PriorityQueue<Uuid, Reverse<UnixTimestamp>>,
}

impl ExpirationHandler {
    pub fn new(
        cancellation_request_sender: Sender<OrderRequest>,
        expiration_order_request_reciever: Receiver<ExpirationOrderRequest>,
    ) -> Self {
        Self {
            cancellation_request_sender,
            expiration_order_request_reciever,
            expiration_queue: PriorityQueue::new(),
        }
    }

    fn remove_expiration_request(&mut self, order_id: Uuid) {
        if self.expiration_queue.get_priority(&order_id).is_some() {
            self.expiration_queue.remove(&order_id);
        }
    }

    pub fn run(&mut self) {
        loop {
            if let Ok(expiration_order_request) = self.expiration_order_request_reciever.try_recv()
            {
                match expiration_order_request {
                    ExpirationOrderRequest::InsertExpirationRequest(insert_expiration_request) => {
                        let _ = self.insert_expiring_order(insert_expiration_request);
                    }
                    ExpirationOrderRequest::RemoveExpirationRequest(order_id) => {
                        self.remove_expiration_request(order_id)
                    }
                }
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
        order_expiration_request: InsertExpirationRequest,
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use crossbeam::channel;
    use uuid::Uuid;

    #[test]
    fn timestamps_occurring_sooner_given_higher_priority() {
        let (_, rx) = channel::unbounded();
        let (cancel_tx, _cancel_rx) = channel::unbounded();
        let mut handler = ExpirationHandler::new(cancel_tx, rx);

        let order_id_1 = Uuid::new_v4();
        let timestamp = (Utc::now() + Duration::seconds(100)).timestamp();
        let order_expiration_request = InsertExpirationRequest {
            order_id: order_id_1,
            timestamp,
        };

        handler
            .insert_expiring_order(order_expiration_request)
            .unwrap();

        let order_id_2 = Uuid::new_v4();
        let timestamp = (Utc::now() + Duration::seconds(1)).timestamp();
        let order_expiration_request = InsertExpirationRequest {
            order_id: order_id_2,
            timestamp,
        };

        handler
            .insert_expiring_order(order_expiration_request)
            .unwrap();

        assert_eq!(handler.expiration_queue.pop().unwrap().0, order_id_2);
    }

    #[test]
    fn test_insert_expiring_order() {
        let (_, rx) = channel::unbounded();
        let (cancel_tx, _cancel_rx) = channel::unbounded();
        let mut handler = ExpirationHandler::new(cancel_tx, rx);

        let order_id = Uuid::new_v4();
        let timestamp = (Utc::now() + Duration::seconds(2)).timestamp();
        let order_expiration_request = InsertExpirationRequest {
            order_id,
            timestamp,
        };

        assert!(handler
            .insert_expiring_order(order_expiration_request)
            .is_ok());
        assert_eq!(handler.expiration_queue.len(), 1);
    }

    #[test]
    fn test_insert_expiring_order_with_past_timestamp() {
        let (_, rx) = channel::unbounded();
        let (cancel_tx, _cancel_rx) = channel::unbounded();
        let mut handler = ExpirationHandler::new(cancel_tx, rx);

        let order_id = Uuid::new_v4();
        let timestamp = (Utc::now() - Duration::seconds(60)).timestamp();
        let order_expiration_request = InsertExpirationRequest {
            order_id,
            timestamp,
        };

        assert!(handler
            .insert_expiring_order(order_expiration_request)
            .is_err());
        assert_eq!(handler.expiration_queue.len(), 0);
    }

    #[test]
    fn test_send_cancellation_request() {
        let (_, rx) = channel::unbounded();
        let (cancel_tx, cancel_rx) = channel::unbounded();
        let mut handler = ExpirationHandler::new(cancel_tx, rx);

        let order_uuid = Uuid::new_v4();
        assert!(handler.send_cancellation_request(order_uuid).is_ok());

        match cancel_rx.try_recv() {
            Ok(OrderRequest::Cancel(CancelRequestType::Internal, received_uuid)) => {
                assert_eq!(received_uuid, order_uuid);
            }
            _ => panic!("Did not receive expected cancellation request"),
        }
    }

    #[test]
    fn test_cancelling_expiration_request() {
        let (_, rx) = channel::unbounded();
        let (cancel_tx, _) = channel::unbounded();
        let mut handler = ExpirationHandler::new(cancel_tx, rx);

        let order_id = Uuid::new_v4();
        let timestamp = (Utc::now() + Duration::seconds(100)).timestamp();
        let order_expiration_request = InsertExpirationRequest {
            order_id,
            timestamp,
        };

        handler
            .insert_expiring_order(order_expiration_request)
            .unwrap();

        handler.remove_expiration_request(order_id);

        assert!(handler.expiration_queue.is_empty())
    }
}
