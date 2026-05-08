use crossbeam::channel::Receiver;
use log::{info, warn};
use uuid::Uuid;

use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::thread;
use std::time::Duration;

use crate::common::error::Result;
use crate::common::protocol::*;
use crate::server::quote::{StockQuote, StockQuoteSource};
use crate::server::subscription::{Subscription, SubscriptionManager, SubscriptionManagerError};

const TCP_TTL_MS: u32 = 100;
const CLIENT_TIMEOUT_FACTOR: u32 = 4;

pub struct QuoteServerConfig {
    pub ip_addr: IpAddr,
    pub tcp_port: u16,
    pub udp_port: u16,

    pub client_timeout_ms: u32,
    pub update_period_ms: u32,

    pub tickers: Vec<String>,
}

pub struct QuoteServer {
    ip_addr: IpAddr,
    tcp_port: u16,
    udp_port: u16,

    client_timeout_ms: u32,
    update_period_ms: u32,

    subscription_mgr: SubscriptionManager<StockQuote>,
}

impl QuoteServer {
    pub fn new(config: &QuoteServerConfig) -> QuoteServer {
        let subscription_mgr = SubscriptionManager::new(&config.tickers, config.client_timeout_ms);

        return QuoteServer {
            ip_addr: config.ip_addr,
            tcp_port: config.tcp_port,
            udp_port: config.udp_port,
            client_timeout_ms: config.client_timeout_ms,
            update_period_ms: config.update_period_ms,
            subscription_mgr,
        };
    }

    pub fn run(&mut self) -> Result<()> {
        let udp_socket_addr = SocketAddr::new(self.ip_addr, self.udp_port);
        let udp_socket = UdpSocket::bind(udp_socket_addr)?;

        let tcp_socket_addr = SocketAddr::new(self.ip_addr, self.tcp_port);
        let tcp_listener = TcpListener::bind(tcp_socket_addr)?;

        tcp_listener.set_ttl(TCP_TTL_MS)?;

        self.start_processing_refresh_events(&udp_socket)?;
        self.start_quote_update();

        info!("Started listening TCP {tcp_socket_addr}");

        loop {
            match self.accept_incoming_connection(&tcp_listener, &udp_socket) {
                Ok(client_addr) => info!("Accepted incoming connection from {client_addr}"),
                Err(e) => warn!("Failed to accept incoming connection: {e:?}"),
            }
        }
    }

    fn start_quote_update(&self) {
        let subscription_mgr = self.subscription_mgr.clone();
        let update_period_ms = self.update_period_ms;

        thread::spawn(move || {
            Self::process_quote_update(subscription_mgr, update_period_ms);
        });
    }

    fn process_quote_update(
        mut subscription_mgr: SubscriptionManager<StockQuote>,
        update_period_ms: u32,
    ) {
        let mut quote_src = StockQuoteSource::new(subscription_mgr.topics_supported());

        'outer: loop {
            quote_src.update();

            for (ticker, quote) in quote_src.quotes_by_ticker() {
                if let Err(SubscriptionManagerError::ShuttingDown) =
                    subscription_mgr.send_event(ticker, quote)
                {
                    break 'outer;
                }
            }

            thread::sleep(Duration::from_millis(update_period_ms as u64));
        }
    }

    fn start_processing_refresh_events(&self, udp_socket: &UdpSocket) -> Result<()> {
        let udp_socket = udp_socket.try_clone()?;
        let subscription_mgr = self.subscription_mgr.clone();

        thread::spawn(move || {
            Self::process_refresh_subscription_events(udp_socket, subscription_mgr);
        });

        return Ok(());
    }

    fn process_refresh_subscription_events(
        udp_socket: UdpSocket,
        mut subscription_mgr: SubscriptionManager<StockQuote>,
    ) {
        let udp_addr = udp_socket.local_addr().unwrap();
        info!("Started listening UPD {udp_addr}");

        loop {
            match read_client_udp_message(&udp_socket) {
                Ok(ClientUdpMessage::Ping { session_id }) => {
                    if let Err(SubscriptionManagerError::ShuttingDown) =
                        subscription_mgr.refresh_subscription(&session_id)
                    {
                        break;
                    }
                }
                Err(e) => {
                    warn!("Failed to accept incoming client message: {e:?}");
                }
            }
        }
    }

    fn accept_incoming_connection(
        &mut self,
        tcp_listener: &TcpListener,
        udp_socket: &UdpSocket,
    ) -> Result<SocketAddr> {
        let (tcp_stream, client_tcp_socket_addr) = tcp_listener.accept()?;

        let client_message = read_client_tcp_message(tcp_stream.try_clone()?)?;

        match client_message {
            ClientTcpMessage::OpenSubscriptionRequest {
                tickers_subscribed,
                client_udp_port,
            } => {
                let client_udp_addr = SocketAddr::new(client_tcp_socket_addr.ip(), client_udp_port);

                self.open_subscription(
                    tcp_stream.try_clone()?,
                    udp_socket.try_clone()?,
                    client_udp_addr.clone(),
                    tickers_subscribed,
                )?;

                return Ok(client_udp_addr);
            }
        }
    }

    fn open_subscription(
        &mut self,
        tcp_stream: TcpStream,
        udp_socket: UdpSocket,
        client_udp_addr: SocketAddr,
        tickers_subscribed: Vec<String>,
    ) -> Result<()> {
        return match self.subscription_mgr.subscribe(tickers_subscribed) {
            Ok(subscription) => {
                self.start_event_processing(tcp_stream, udp_socket, client_udp_addr, subscription)
            }
            Err(e) => Self::response_subscription_error(tcp_stream, e),
        };
    }

    fn start_event_processing(
        &self,
        tcp_stream: TcpStream,
        udp_socket: UdpSocket,
        client_udp_addr: SocketAddr,
        subscription: Subscription<StockQuote>,
    ) -> Result<()> {
        thread::spawn(move || {
            Self::process_subscribed_events(
                udp_socket,
                client_udp_addr,
                subscription.session_id,
                subscription.event_receiver,
            )
        });

        return write_server_tcp_message(
            tcp_stream,
            &ServerTcpMessage::OpenSubscriptionSucceed {
                session_id: subscription.session_id,
                server_udp_port: self.udp_port,
                ping_timeout_ms: self.client_timeout_ms / CLIENT_TIMEOUT_FACTOR,
                update_period_ms: self.update_period_ms,
            },
        );
    }

    fn process_subscribed_events(
        client_udp_socket: UdpSocket,
        client_udp_addr: SocketAddr,
        client_session_id: Uuid,
        event_receiver: Receiver<(String, StockQuote)>,
    ) {
        loop {
            match event_receiver.recv() {
                Ok((ticker, quote)) => {
                    let message = &ServerUdpMessage::Quote {
                        session_id: client_session_id,
                        ticker: ticker,
                        price: quote.price,
                        volume: quote.volume,
                        timestamp: quote.timestamp,
                    };

                    if let Err(e) =
                        write_server_udp_message(&client_udp_socket, &client_udp_addr, message)
                    {
                        warn!("Failed to send quote: {e:?}");
                    }
                }
                Err(_) => break,
            }
        }
    }

    fn response_subscription_error(
        tcp_stream: TcpStream,
        error: SubscriptionManagerError,
    ) -> Result<()> {
        let error_cause = match error {
            SubscriptionManagerError::EmptyTopics => OpenSubscriptionError::EmptyTickers,
            SubscriptionManagerError::UnknownTopics { topics } => {
                OpenSubscriptionError::UnknownTickers { tickers: topics }
            }
            SubscriptionManagerError::ShuttingDown => OpenSubscriptionError::ShuttingDown,
        };

        return write_server_tcp_message(
            tcp_stream,
            &ServerTcpMessage::OpenSubscriptionFailed { error: error_cause },
        );
    }
}
