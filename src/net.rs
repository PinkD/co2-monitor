use alloc::vec;
use alloc::vec::Vec;
use smoltcp::socket::udp;

pub fn parse_ip(ip: &str) -> [u8; 4] {
    let mut result = [0u8; 4];
    for (idx, octet) in ip.split(".").into_iter().enumerate() {
        result[idx] = u8::from_str_radix(octet, 10).unwrap();
    }
    result
}

pub struct SocketBuff {
    pub rx_meta: Vec<udp::PacketMetadata>,
    pub rx_buffer: [u8; 1500],
    pub tx_meta: Vec<udp::PacketMetadata>,
    pub tx_buffer: [u8; 1500],
}

impl SocketBuff {
    pub fn new() -> SocketBuff {
        SocketBuff {
            tx_meta: vec![udp::PacketMetadata::EMPTY.clone()],
            tx_buffer: [0u8; 1500],
            rx_meta: vec![udp::PacketMetadata::EMPTY.clone()],
            rx_buffer: [0u8; 1500],
        }
    }
}
