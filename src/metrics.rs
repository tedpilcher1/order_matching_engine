use lazy_static::lazy_static;
use prometheus::{register_histogram, register_int_counter, Histogram, IntCounter};

lazy_static! {
    pub static ref ORDER_COUNTER: IntCounter =
        register_int_counter!("order_counter", "Number orders recieved").unwrap();
    pub static ref TRADE_COUNTER: IntCounter =
        register_int_counter!("trade_counter", "Number trades processed").unwrap();
    pub static ref MATCHING_DURATION: Histogram = register_histogram!(
        "matching_duration",
        "Duration to match order with resting order"
    )
    .unwrap();
}
