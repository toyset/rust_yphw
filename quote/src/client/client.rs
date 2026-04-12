use log::{info, warn};
use uuid::Uuid;

use std::error::Error;
use std::fmt::Display;
use std::net::{IpAddr, SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle, sleep};
use std::time::{Duration, Instant};

use crate::common::error::Result;
use crate::common::protocol::*;

const RETRY_SESSION_TIMEOUT_MS: u64 = 2_000;
const SERVER_CONNECT_TIMEOUT_MS: u64 = 2_000;
const TCP_TTL_MS: u32 = 100;
const UDP_READ_TIMEOUT_MS: u64 = 1_000;
const UDP_WRITE_TIMEOUT_MS: u64 = 1_000;

const INCOMING_MESSAGE_TIMEOUT_FACTOR: u64 = 4;

#[derive(Debug)]
pub enum ServerTcpError {
    EmptySubscriptionTickers,
    UnknownSubscriptionTickers { tickers: Vec<String> },
    ServerIsShuttingDown,
    ClientMessageUnrecognized,
}

impl Display for ServerTcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptySubscriptionTickers => write!(
                f,
                "Connection failed, no tickers specified for subscription"
            ),
            Self::UnknownSubscriptionTickers { tickers } => write!(
                f,
                "Connection failed, unknown tickers specified for subscription: {}",
                tickers.join(", ")
            ),
            Self::ServerIsShuttingDown => write!(f, "Connection failed, server is shutting down"),
            Self::ClientMessageUnrecognized => {
                write!(f, "Connection failed, incompartible client version")
            }
        }
    }
}

impl Error for ServerTcpError {}

#[derive(Clone)]
pub struct QuoteClientConfig {
    pub server_ip_addr: IpAddr,
    pub server_tcp_port: u16,

    pub client_ip_addr: IpAddr,
    pub client_udp_port: u16,

    pub tickers: Vec<String>,
}

pub struct QuoteClientHandle {
    client_active: Arc<AtomicBool>,
}

impl QuoteClientHandle {
    pub fn stop(&self) {
        self.client_active.store(false, Ordering::Relaxed);
    }
}

pub struct QuoteClient {
    config: QuoteClientConfig,
    udp_socket: UdpSocket,
    client_active: Arc<AtomicBool>,
}

struct QuoteServerSession {
    session_id: Uuid,
    update_period_ms: u32,
    keepalive_thread_handle: JoinHandle<()>,
    session_active: Arc<AtomicBool>,
}

impl QuoteClient {
    pub fn start(config: &QuoteClientConfig) -> Result<(QuoteClientHandle, JoinHandle<()>)> {
        let udp_socket_addr = SocketAddr::new(config.client_ip_addr, config.client_udp_port);
        let udp_socket = UdpSocket::bind(udp_socket_addr)?;

        udp_socket.set_read_timeout(Some(Duration::from_millis(UDP_READ_TIMEOUT_MS)))?;
        udp_socket.set_write_timeout(Some(Duration::from_millis(UDP_WRITE_TIMEOUT_MS)))?;

        let client_active = Arc::new(AtomicBool::new(true));

        let quote_client = QuoteClient {
            config: config.clone(),
            udp_socket: udp_socket,
            client_active: client_active.clone(),
        };

        let processing_thread_handle = thread::spawn(move || quote_client.process_while_active());

        return Ok((
            QuoteClientHandle { client_active },
            processing_thread_handle,
        ));
    }

    fn process_while_active(self) {
        while self.process_server_session().is_some() {}
    }

    fn check_active(&self) -> Option<()> {
        if self.client_active.load(Ordering::Relaxed) {
            Some(())
        } else {
            None
        }
    }

    fn process_server_session(&self) -> Option<()> {
        let session = self.ensure_session_opened()?;
        let result = self.accept_incoming_messages(&session);

        session.deinit();

        return result;
    }

    fn ensure_session_opened(&self) -> Option<QuoteServerSession> {
        loop {
            self.check_active()?;

            match self.try_open_session() {
                Ok(session) => return Some(session),
                Err(e) => {
                    warn!("Failed to open server session: {e:?}");
                    self.check_active()?;
                    thread::sleep(Duration::from_millis(RETRY_SESSION_TIMEOUT_MS));
                }
            }
        }
    }

    fn try_open_session(&self) -> Result<QuoteServerSession> {
        let subscription_request = ClientTcpMessage::OpenSubscriptionRequest {
            tickers_subscribed: self.config.tickers.clone(),
            client_udp_port: self.config.client_udp_port,
        };

        let server_tcp_addr =
            SocketAddr::new(self.config.server_ip_addr, self.config.server_tcp_port);
        let server_tcp_stream = TcpStream::connect_timeout(
            &server_tcp_addr,
            Duration::from_millis(SERVER_CONNECT_TIMEOUT_MS),
        )?;

        server_tcp_stream.set_ttl(TCP_TTL_MS)?;

        write_client_tcp_message(server_tcp_stream.try_clone()?, &subscription_request)?;
        let subscription_response = read_server_tcp_message(server_tcp_stream)?;

        let session = match subscription_response {
            ServerTcpMessage::OpenSubscriptionSucceed {
                session_id,
                server_udp_port,
                ping_timeout_ms,
                update_period_ms,
            } => self.init_session(
                session_id,
                server_udp_port,
                ping_timeout_ms,
                update_period_ms,
            )?,
            ServerTcpMessage::OpenSubscriptionFailed { error } => {
                Err(Self::map_server_open_subscription_error(error))?
            }
            ServerTcpMessage::ClientMessageUnrecognized => {
                Err(ServerTcpError::ClientMessageUnrecognized)?
            }
        };

        return Ok(session);
    }

    fn map_server_open_subscription_error(error: OpenSubscriptionError) -> ServerTcpError {
        match error {
            OpenSubscriptionError::EmptyTickers => ServerTcpError::EmptySubscriptionTickers,
            OpenSubscriptionError::UnknownTickers { tickers } => {
                ServerTcpError::UnknownSubscriptionTickers { tickers }
            }
            OpenSubscriptionError::ShuttingDown => ServerTcpError::ServerIsShuttingDown,
        }
    }

    fn init_session(
        &self,
        session_id: Uuid,
        server_udp_port: u16,
        ping_timeout_ms: u32,
        update_period_ms: u32,
    ) -> Result<QuoteServerSession> {
        let server_udp_addr = SocketAddr::new(self.config.server_ip_addr, server_udp_port);

        info!("Connecting UDP socket to {server_udp_addr}");
        self.udp_socket.connect(server_udp_addr)?;

        return Ok(QuoteServerSession::init(
            self.udp_socket.try_clone()?,
            session_id,
            ping_timeout_ms,
            update_period_ms,
        ));
    }

    fn accept_incoming_messages(&self, session: &QuoteServerSession) -> Option<()> {
        let incoming_message_timeout = Duration::from_millis(
            session.update_period_ms as u64 * INCOMING_MESSAGE_TIMEOUT_FACTOR,
        );

        let mut last_message_instant = Instant::now();

        while last_message_instant.elapsed() < incoming_message_timeout {
            match read_server_udp_message(&self.udp_socket) {
                Ok(message) => {
                    if Self::map_udp_server_message(session, message) {
                        last_message_instant = Instant::now();
                    }
                }
                Err(e) => warn!("Failed to accept message from server: {e}"),
            }

            self.check_active()?;
        }

        return Some(());
    }

    fn map_udp_server_message(session: &QuoteServerSession, message: ServerUdpMessage) -> bool {
        match message {
            ServerUdpMessage::Quote {
                session_id,
                ticker,
                price,
                volume,
                timestamp,
            } => {
                if session_id != session.session_id {
                    return false;
                }

                println!(
                    "ticker: {ticker}; price: {price}; volume: {volume}; timestamp: {timestamp}"
                );
            }
            ServerUdpMessage::TimedOut => return false,
        };

        return true;
    }
}

impl QuoteServerSession {
    pub fn init(
        udp_socket: UdpSocket,
        session_id: Uuid,
        ping_timeout_ms: u32,
        update_period_ms: u32,
    ) -> Self {
        let session_active = Arc::new(AtomicBool::new(true));
        let session_active_cloned = session_active.clone();

        let keepalive_thread_handle = thread::spawn(move || {
            Self::process_session_keepalive(
                udp_socket,
                ping_timeout_ms,
                session_id,
                session_active,
            );
        });

        return QuoteServerSession {
            session_id,
            update_period_ms,
            keepalive_thread_handle,
            session_active: session_active_cloned,
        };
    }

    fn process_session_keepalive(
        udp_socket: UdpSocket,
        ping_timeout_ms: u32,
        session_id: Uuid,
        session_active: Arc<AtomicBool>,
    ) {
        let ping_message = ClientUdpMessage::Ping { session_id };

        while session_active.load(Ordering::Relaxed) {
            if let Err(e) = write_client_udp_message(&udp_socket, &ping_message) {
                warn!("Failed to send keepalive packet: {e}");
            }

            sleep(Duration::from_millis(ping_timeout_ms as u64));
        }
    }

    pub fn deinit(self) {
        self.session_active.store(false, Ordering::Relaxed);
        _ = self.keepalive_thread_handle.join();
    }
}
