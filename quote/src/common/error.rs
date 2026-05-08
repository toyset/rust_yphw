use std::error::Error as StdError;
use std::result::Result as StdResult;

pub type Error = Box<dyn StdError + Send + Sync + 'static>;
pub type Result<T> = StdResult<T, Error>;
