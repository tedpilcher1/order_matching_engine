use lazy_static::lazy_static;
use prometheus::{register_histogram, register_int_counter, Histogram, IntCounter, Registry};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();
    pub static ref ORDERS_FILLED_COUNTER: IntCounter =
        register_int_counter!("orders_filled_counter", "Number orders filled").unwrap();
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

pub fn register_custom_metrics() {
    REGISTRY
        .register(Box::new(ORDERS_FILLED_COUNTER.clone()))
        .expect("collector can be registered");

    REGISTRY
        .register(Box::new(ORDER_COUNTER.clone()))
        .expect("collector can be registered");

    REGISTRY
        .register(Box::new(TRADE_COUNTER.clone()))
        .expect("collector can be registered");

    REGISTRY
        .register(Box::new(MATCHING_DURATION.clone()))
        .expect("collector can be registered");
}
