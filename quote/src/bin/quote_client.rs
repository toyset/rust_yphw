use quote::client::client::{QuoteClient, QuoteClientConfig};
use quote::common::error::Result;

use clap::Parser;

use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// IP адрес сервера
    #[arg(long, default_value = "127.0.0.1")]
    server_ip: IpAddr,

    /// TCP порт сервера
    #[arg(long, default_value_t = 34254)]
    server_port: u16,

    /// IP адрес клиента
    #[arg(long, default_value = "127.0.0.1")]
    client_ip: IpAddr,

    /// UDP порт клиента
    #[arg(long, default_value_t = 34256)]
    client_port: u16,

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

    let config = QuoteClientConfig {
        server_ip_addr: args.server_ip,
        server_tcp_port: args.server_port,
        client_ip_addr: args.client_ip,
        client_udp_port: args.client_port,
        tickers,
    };

    let (client_handle, client_thread_handle) = QuoteClient::start(&config)?;

    ctrlc::set_handler(move || client_handle.stop())?;

    _ = client_thread_handle.join();

    return Ok(());
}
