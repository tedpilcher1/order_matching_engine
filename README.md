# Order Matching Engine

Open source, highly performant, order matching engine written in Rust.

## Order Requests

- Create
- Cancel
- Modify
  - Cancels existing order & creates new order
  - Cannot modify side or type
  - If remaining quantity of existing order >= new minimum quantity, new order will not be created

## Supported Order Properties

The engine supports the following properties:

- Price
- Quantity
- Side: Buy or Sell
- Minimum Quantity
  - Order will only be filled if quantity >= minimum quantity
- Expiration Date:
  - Cancels order at specified date
- Type: Normal or Kill
  - Kill orders will not enter the order book as a resting order

## Order Types

- Limit Orders
- Good-Until-Date
  - Specify some expiratation date
- Good-Till-Canceled
  - Call the cancellation endpoint with the order's id
- Fill-Or-Kill
  - Set minimum quantity to quantity and type to kill
- Fill-And-Kill
  - Specify type as kill

## Endpoints

| HTTP Method | Endpoint                  | JSON Request Body |
| ----------- | ------------------------- | ----------------- |
| POST        | `/create_order`           | `TradeRequest`    |
| POST        | `/cancel_order{order_id}` | None              |
| POST        | `/modify_order`           | `TradeRequest`    |

#### `TradeRequest`:

```json
{
  "id": "UUID",
  "order_type": "Normal|Kill",
  "order_side": "Buy|Sell",
  "price": "f64",
  "quantity": "u64",
  "minimum_quantity": "u64",
  "expiration_date": "DateTime|null"
}
```

## Performance

### Load testing

To simulate some basic usage for load testing, first ensure the `web_server` binary is running and then run the following command:

```console
cargo run --bin load_tester --release -- -H http://127.0.0.1:8080/ --startup-time 1m --users 50 --run-time 30m --no-reset-metrics
```

## Usage

Run the following command to build & run the binary:

```console
cargo run --release --bin web_server
```
