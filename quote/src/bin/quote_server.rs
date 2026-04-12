use quote::common::error::Result;
use quote::server::server::{QuoteServer, QuoteServerConfig};

use clap::Parser;

use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// IP адрес сервера
    #[arg(long, default_value = "127.0.0.1")]
    ip: IpAddr,

    /// TCP порт сервера
    #[arg(long, default_value_t = 34254)]
    tcp_port: u16,

    /// UDP порт клиента
    #[arg(long, default_value_t = 34255)]
    udp_port: u16,

    /// Максимальное время ожидания пинга от клиента (мс)
    #[arg(long, default_value_t = 2000)]
    client_timeout_ms: u32,

    /// Период обновления котировок (мс)
    #[arg(long, default_value_t = 2000)]
    update_period_ms: u32,

    /// Путь к файлу с тикерами
    #[arg(short = 'f', long)]
    tickers_file: PathBuf,
}

fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();

    let tickers_content = fs::read_to_string(&args.tickers_file)?;

    let tickers: Vec<String> = tickers_content
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    if tickers.is_empty() {
        eprintln!("Файл тикеров пуст или содержит только пустые строки");
        return Ok(());
    }

    let server_config = QuoteServerConfig {
        ip_addr: args.ip,
        tcp_port: args.tcp_port,
        udp_port: args.udp_port,

        client_timeout_ms: args.client_timeout_ms,
        update_period_ms: args.update_period_ms,

        tickers: tickers,
    };

    let mut server = QuoteServer::new(&server_config);

    return server.run();
}
