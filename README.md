## Load testing

cargo run --bin load_tester --release -- -H http://127.0.0.1:8080/ --startup-time 1m --users 200 --run-time 30m --no-reset-metrics

## Prometheus

docker run -p 9090:9090 prom/prometheus
