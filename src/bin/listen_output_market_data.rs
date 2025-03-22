use anyhow::Result;
use borsh::BorshDeserialize;
use order_matching_engine::market_data_outbox::expose_trade_worker::{
    MULTICAST_ADDR, MULTICAST_PORT,
};
use order_matching_engine::orderbook::MarketDataUpdate;
use socket2::{Domain, Protocol, Socket, Type};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::net::UdpSocket;

const BUFFER_SIZE: usize = 1024;

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting Market Data Listener...");
    println!(
        "Listening for trades on {}:{}",
        MULTICAST_ADDR, MULTICAST_PORT
    );

    // Set up the multicast receiver socket
    let socket = setup_multicast_socket()?;
    let socket = Arc::new(socket);

    // Buffer to receive data
    let mut buf = vec![0u8; BUFFER_SIZE];

    // Main receive loop
    println!("Waiting for trade updates...");
    loop {
        let (size, _src_addr) = socket.recv_from(&mut buf).await?;

        // Try to deserialize the received data
        match MarketDataUpdate::try_from_slice(&buf[..size]) {
            Ok(trade) => {
                println!("Received trade: {:#?}", trade);
                println!("---------------------------------------------------");
            }
            Err(e) => {
                eprintln!("Error deserializing trade data: {}", e);
            }
        }
    }
}

fn setup_multicast_socket() -> Result<UdpSocket> {
    // Create a socket
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

    // Set socket options
    socket.set_reuse_address(true)?;

    // Bind to the multicast port
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), MULTICAST_PORT);
    socket.bind(&addr.into())?;

    // Join the multicast group
    socket.join_multicast_v4(&MULTICAST_ADDR, &Ipv4Addr::UNSPECIFIED)?;

    // Convert to tokio UDP socket
    let std_socket = std::net::UdpSocket::from(socket);
    std_socket.set_nonblocking(true)?;

    Ok(UdpSocket::from_std(std_socket)?)
}
