use goose::prelude::*;
use order_matching_engine::orderbook::{OrderSide, OrderType};
use uuid::Uuid;

async fn add_order(user: &mut GooseUser) -> TransactionResult {
    let order_side = if rand::random_bool(0.5) {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };

    let price = rand::random_range(1..10);
    let quantity = rand::random_range(1..10);

    let body = &serde_json::json!({
        "id": Uuid::new_v4(),
        "order_type": OrderType::Normal,
        "order_side": order_side,
        "price": price,
        "quantity": quantity,
        "minimum_quantity": 0, // not implemented yet
    });

    let _ = user.post_json("create_order", &body).await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), GooseError> {
    GooseAttack::initialize()?
        .register_scenario(scenario!("APIUser").register_transaction(transaction!(add_order)))
        .execute()
        .await?;

    Ok(())
}
