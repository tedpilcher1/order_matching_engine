use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Result;

use crate::orderbook::MarketDataUpdate;
use borsh::BorshSerialize;
use crossbeam::channel::Receiver;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

pub const MULTICAST_PORT: u16 = 8888;
pub const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 10, 10);

pub struct MarketDataWorker {
    trade_reciever: Receiver<MarketDataUpdate>,
    socket: UdpSocket,
}

impl MarketDataWorker {
    pub fn new(trade_reciever: Receiver<MarketDataUpdate>) -> Self {
        let socket = MarketDataWorker::setup_socket().expect("Should be able to create socket");
        Self {
            trade_reciever,
            socket,
        }
    }

    fn setup_socket() -> Result<UdpSocket> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_reuse_address(true)?;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        socket.bind(&addr.into())?;
        socket.set_multicast_ttl_v4(5)?;
        Ok(UdpSocket::from_std(std::net::UdpSocket::from(socket))?)
    }

    pub async fn do_work(&mut self) {
        let dest_addr = SocketAddr::new(IpAddr::V4(MULTICAST_ADDR), MULTICAST_PORT);
        println!("Waiting to recieve market data");
        loop {
            if let Ok(trade) = self.trade_reciever.recv() {
                println!("recieved trade: {:?}", trade);
                let mut buffer: Vec<u8> = Vec::new();
                if trade.serialize(&mut buffer).is_ok() {
                    let _ = self.socket.send_to(&buffer, &dest_addr).await;
                }
            }
        }
    }
}
