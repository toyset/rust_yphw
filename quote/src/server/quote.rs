use chrono::Utc;
use rand::{Rng, rngs::ThreadRng};
use std::cmp::{max, min};
use std::collections::HashMap;

const PRICE_STEP: i64 = 100;
const PRICE_MIN: u64 = 1_000;
const PRICE_MAX: u64 = 1_000_000;

const VOLUME_STEP: i32 = 10;
const VOLUME_MIN: u32 = 100;
const VOLUME_MAX: u32 = 100_000;

#[derive(Clone)]
pub struct StockQuote {
    pub price: u64,
    pub volume: u32,
    pub timestamp: i64,
}

pub struct StockQuoteSource {
    rng: ThreadRng,
    quotes_by_ticker: HashMap<String, StockQuote>,
}

impl StockQuoteSource {
    pub fn new<'a, I>(tickers: &'a I) -> Self
    where
        &'a I: IntoIterator<Item = &'a String>,
    {
        let mut quotes_by_ticker = HashMap::new();
        let mut rng = rand::thread_rng();

        for ticker in tickers {
            quotes_by_ticker.insert(ticker.clone(), Self::random_quote(&mut rng));
        }

        return StockQuoteSource {
            rng,
            quotes_by_ticker,
        };
    }

    fn random_quote(rng: &mut ThreadRng) -> StockQuote {
        StockQuote {
            price: rng.gen_range(PRICE_MIN..PRICE_MAX),
            volume: rng.gen_range(VOLUME_MIN..VOLUME_MAX),
            timestamp: 0,
        }
    }

    fn update_quote(rng: &mut ThreadRng, timestamp: i64, quote: &mut StockQuote) {
        let price = quote.price as i64 + rng.gen_range(-PRICE_STEP..PRICE_STEP);
        let price_adjusted = min(PRICE_MAX, max(PRICE_MIN, price as u64));

        let volume = quote.volume as i32 + rng.gen_range(-VOLUME_STEP..VOLUME_STEP);
        let volume_adjusted = min(VOLUME_MAX, max(VOLUME_MIN, volume as u32));

        quote.price = price_adjusted;
        quote.volume = volume_adjusted;
        quote.timestamp = timestamp;
    }

    pub fn update(&mut self) {
        let timestamp = Utc::now().timestamp();

        for quote in self.quotes_by_ticker.values_mut() {
            Self::update_quote(&mut self.rng, timestamp, quote);
        }
    }

    pub fn get(&self, ticker: &String) -> Option<&StockQuote> {
        self.quotes_by_ticker.get(ticker)
    }

    pub fn quotes_by_ticker(&self) -> &HashMap<String, StockQuote> {
        &self.quotes_by_ticker
    }
}
