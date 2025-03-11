use crossbeam::channel::Sender;

use crate::web_server::OrderRequest;

pub struct ExpirationHandler {
    pub sender: Sender<OrderRequest>, // for cancelling orders
}
