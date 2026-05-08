use crate::common::error::Result;

use serde::{Deserialize, Serialize};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, UdpSocket};

use uuid::Uuid;

#[derive(Deserialize, Serialize)]
pub enum ClientTcpMessage {
    OpenSubscriptionRequest {
        tickers_subscribed: Vec<String>,
        client_udp_port: u16,
    },
}

pub fn read_client_tcp_message<R: Read>(reader: R) -> Result<ClientTcpMessage> {
    Ok(bincode::deserialize_from(reader)?)
}

pub fn write_client_tcp_message<W: Write>(writer: W, message: &ClientTcpMessage) -> Result<()> {
    Ok(bincode::serialize_into(writer, message)?)
}

#[derive(Deserialize, Serialize)]
pub enum OpenSubscriptionError {
    EmptyTickers,
    UnknownTickers { tickers: Vec<String> },
    ShuttingDown,
}

#[derive(Deserialize, Serialize)]
pub enum ServerTcpMessage {
    OpenSubscriptionSucceed {
        session_id: Uuid,
        server_udp_port: u16,
        ping_timeout_ms: u32,
        update_period_ms: u32,
    },
    OpenSubscriptionFailed {
        error: OpenSubscriptionError,
    },
    ClientMessageUnrecognized,
}

pub fn read_server_tcp_message<R: Read>(reader: R) -> Result<ServerTcpMessage> {
    Ok(bincode::deserialize_from(reader)?)
}

pub fn write_server_tcp_message<W: Write>(writer: W, message: &ServerTcpMessage) -> Result<()> {
    Ok(bincode::serialize_into(writer, message)?)
}

#[derive(Deserialize, Serialize)]
pub enum ClientUdpMessage {
    Ping { session_id: Uuid },
}

const CLIENT_UDP_BUFFER_SIZE: usize = 256;

pub fn read_client_udp_message(socket: &UdpSocket) -> Result<ClientUdpMessage> {
    let mut buffer = [0u8; CLIENT_UDP_BUFFER_SIZE];
    let (bytes_read, _) = socket.recv_from(&mut buffer)?;

    return Ok(bincode::deserialize(&buffer[..bytes_read])?);
}

pub fn write_client_udp_message(socket: &UdpSocket, message: &ClientUdpMessage) -> Result<()> {
    let buffer = bincode::serialize(message)?;
    socket.send(&buffer)?;

    return Ok(());
}

#[derive(Deserialize, Serialize)]
pub enum ServerUdpMessage {
    Quote {
        session_id: Uuid,
        ticker: String,
        price: u64,
        volume: u32,
        timestamp: i64,
    },
    TimedOut,
}

const SERVER_UDP_BUFFER_SIZE: usize = 1024;

pub fn read_server_udp_message(socket: &UdpSocket) -> Result<ServerUdpMessage> {
    let mut buffer = [0u8; SERVER_UDP_BUFFER_SIZE];

    let bytes_read = match socket.recv(&mut buffer) {
        Ok(bytes_read) => bytes_read,
        Err(e) if e.kind() == ErrorKind::TimedOut || e.kind() == ErrorKind::WouldBlock => {
            return Ok(ServerUdpMessage::TimedOut);
        }
        Err(e) => return Err(Box::new(e)),
    };

    return Ok(bincode::deserialize(&buffer[..bytes_read])?);
}

pub fn write_server_udp_message(
    socket: &UdpSocket,
    client_socket_addr: &SocketAddr,
    message: &ServerUdpMessage,
) -> Result<()> {
    let buffer = bincode::serialize(message)?;
    socket.send_to(&buffer, client_socket_addr)?;

    return Ok(());
}
