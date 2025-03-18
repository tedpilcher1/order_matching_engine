use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::Result;
use bincode::{
    config::{self, Configuration},
    encode_to_vec,
};
use crossbeam::channel::Receiver;
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::UdpSocket;

use crate::orderbook::Trade;

const MUTLICAST_PORT: u16 = 8888;

pub struct ExposeTradeWorker {
    trade_reciever: Receiver<Trade>,
    socket: UdpSocket,
    addr: Ipv4Addr,
    encode_config: Configuration,
}

impl ExposeTradeWorker {
    pub fn new(trade_reciever: Receiver<Trade>) -> Self {
        let socket = ExposeTradeWorker::setup_socket().expect("Should be able to create socket");
        Self {
            trade_reciever,
            socket,
            addr: Ipv4Addr::new(239, 255, 10, 10),
            encode_config: config::standard(),
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
        let dest_addr = SocketAddr::new(IpAddr::V4(self.addr), MUTLICAST_PORT);
        loop {
            if let Ok(trade) = self.trade_reciever.recv() {
                if let Ok(encoded_trade) = encode_to_vec(trade, self.encode_config) {
                    let _ = self.socket.send_to(&encoded_trade, &dest_addr).await;
                }
            }
        }
    }
}
