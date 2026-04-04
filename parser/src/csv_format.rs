use csv::{Reader, ReaderBuilder, StringRecord, Writer, WriterBuilder};
use std::io::{BufRead, Error, ErrorKind, Result, Write};
use std::str::FromStr;

use serde::Serialize;

use crate::io::{TransactionRead, TransactionWrite};
use crate::transaction::{Transaction, TransactionStatus, TransactionType};

const COLUMN_ID: usize = 0;
const COLUMN_TYPE: usize = 1;
const COLUMN_FROM_USER_ID: usize = 2;
const COLUMN_TO_USER_ID: usize = 3;
const COLUNM_AMOUNT: usize = 4;
const COLUMN_TIMESTAMP: usize = 5;
const COLUMN_STATUS: usize = 6;
const COLUMN_DESCRIPTION: usize = 7;

const HEADER_ID: &str = "TX_ID";
const HEADER_TYPE: &str = "TX_TYPE";
const HEADER_FROM_USER_ID: &str = "FROM_USER_ID";
const HEADER_TO_USER_ID: &str = "TO_USER_ID";
const HEADER_AMOUNT: &str = "AMOUNT";
const HEADER_TIMESTAMP: &str = "TIMESTAMP";
const HEADER_STATUS: &str = "STATUS";
const HEADER_DESCRIPTION: &str = "DESCRIPTION";

const TRANSACTION_TYPE_DEPOSIT: &str = "DEPOSIT";
const TRANSACTION_TYPE_TRANSFER: &str = "TRANSFER";
const TRANSACTION_TYPE_WITHDRAWAL: &str = "WITHDRAWAL";

const TRANSACTION_STATUS_SUCCESS: &str = "SUCCESS";
const TRANSACTION_STATUS_FAILURE: &str = "FAILURE";
const TRANSACTION_STATUS_PENDING: &str = "PENDING";

fn error_invalid_data(error_message: String) -> Error {
    Error::new(ErrorKind::InvalidData, error_message)
}

fn err_invalid_data<T>(error_message: String) -> Result<T> {
    Err(error_invalid_data(error_message))
}

fn transaction_type_from_raw(tx_type_raw: &str) -> Result<TransactionType> {
    match tx_type_raw {
        TRANSACTION_TYPE_DEPOSIT => Ok(TransactionType::Deposit),
        TRANSACTION_TYPE_TRANSFER => Ok(TransactionType::Transfer),
        TRANSACTION_TYPE_WITHDRAWAL => Ok(TransactionType::Withdrawal),
        _ => err_invalid_data(format!(
            "Illegal value for transaction type ({tx_type_raw})"
        )),
    }
}

fn transaction_type_to_raw(tx_type: TransactionType) -> &'static str {
    match tx_type {
        TransactionType::Deposit => TRANSACTION_TYPE_DEPOSIT,
        TransactionType::Transfer => TRANSACTION_TYPE_TRANSFER,
        TransactionType::Withdrawal => TRANSACTION_TYPE_WITHDRAWAL,
    }
}

fn transaction_status_from_raw(tx_status_raw: &str) -> Result<TransactionStatus> {
    match tx_status_raw {
        TRANSACTION_STATUS_SUCCESS => Ok(TransactionStatus::Success),
        TRANSACTION_STATUS_FAILURE => Ok(TransactionStatus::Failure),
        TRANSACTION_STATUS_PENDING => Ok(TransactionStatus::Pending),
        _ => err_invalid_data(format!(
            "Illegal value for transaction status ({tx_status_raw})"
        )),
    }
}

fn transaction_status_to_raw(tx_status: TransactionStatus) -> &'static str {
    match tx_status {
        TransactionStatus::Success => TRANSACTION_STATUS_SUCCESS,
        TransactionStatus::Failure => TRANSACTION_STATUS_FAILURE,
        TransactionStatus::Pending => TRANSACTION_STATUS_PENDING,
    }
}

/// Десериализация списка транзакций в csv-формате
pub struct CsvTransactionReader<R: BufRead> {
    input: Reader<R>,
    input_line_buffer: StringRecord,

    header_read: bool,
}

impl<R: BufRead> CsvTransactionReader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input: ReaderBuilder::new().has_headers(false).from_reader(input),
            input_line_buffer: StringRecord::new(),

            header_read: false,
        }
    }

    fn ensure_header_read(&mut self) -> Result<bool> {
        if self.header_read {
            return Ok(true);
        }

        if !self.input.read_record(&mut self.input_line_buffer)? {
            return Ok(false);
        }

        self.check_column_header(COLUMN_ID, HEADER_ID)?;
        self.check_column_header(COLUMN_TYPE, HEADER_TYPE)?;
        self.check_column_header(COLUMN_FROM_USER_ID, HEADER_FROM_USER_ID)?;
        self.check_column_header(COLUMN_TO_USER_ID, HEADER_TO_USER_ID)?;
        self.check_column_header(COLUNM_AMOUNT, HEADER_AMOUNT)?;
        self.check_column_header(COLUMN_TIMESTAMP, HEADER_TIMESTAMP)?;
        self.check_column_header(COLUMN_STATUS, HEADER_STATUS)?;
        self.check_column_header(COLUMN_DESCRIPTION, HEADER_DESCRIPTION)?;

        self.header_read = true;

        return Ok(true);
    }

    fn check_column_header(&self, column_index: usize, column_header: &str) -> Result<()> {
        if let Some(column_value) = self.input_line_buffer.get(column_index)
            && column_value == column_header
        {
            return Ok(());
        }

        return err_invalid_data(format!(
            "Expected header \"{}\" at column {}",
            column_header,
            column_index + 1
        ));
    }

    fn try_parse_field<V>(
        &mut self,
        field_index: usize,
        parse: impl Fn(&str) -> Result<V>,
    ) -> Result<V> {
        parse(self.input_line_buffer.get(field_index).unwrap_or(""))
    }

    fn try_parse_u64_field(&mut self, field_index: usize, field_title: &str) -> Result<u64> {
        self.try_parse_field(field_index, |field_value| {
            u64::from_str(field_value).map_err(|e| {
                error_invalid_data(format!("Failed to parse {field_title} value: {e}"))
            })
        })
    }
}

impl<R: BufRead> TransactionRead for CsvTransactionReader<R> {
    fn read_next(&mut self) -> Result<Option<Transaction>> {
        let header_read = self.ensure_header_read()?;
        if !header_read {
            return Ok(None);
        }

        let row_read = self.input.read_record(&mut self.input_line_buffer)?;
        if !row_read {
            return Ok(None);
        }

        let id = self.try_parse_u64_field(COLUMN_ID, "id")?;
        let tx_type = self.try_parse_field(COLUMN_TYPE, |field_value| {
            transaction_type_from_raw(field_value)
        })?;
        let from_user_id = self.try_parse_u64_field(COLUMN_FROM_USER_ID, "from_user_id")?;
        let to_user_id = self.try_parse_u64_field(COLUMN_TO_USER_ID, "to_user_id")?;
        let amount = self.try_parse_u64_field(COLUNM_AMOUNT, "amount")?;
        let timestamp = self.try_parse_u64_field(COLUMN_TIMESTAMP, "timestamp")?;
        let tx_status = self.try_parse_field(COLUMN_STATUS, |field_value| {
            transaction_status_from_raw(field_value)
        })?;
        let description = self
            .input_line_buffer
            .get(COLUMN_DESCRIPTION)
            .unwrap_or("")
            .to_string();

        return Ok(Some(Transaction::new(
            id,
            tx_type,
            tx_status,
            timestamp,
            from_user_id,
            to_user_id,
            amount,
            description,
        )));
    }
}

/// Сериализация списка транзакций в csv-формат
pub struct CsvTransactionWriter<W: Write> {
    output: Writer<W>,
}

impl<W: Write> CsvTransactionWriter<W> {
    pub fn new(output: W) -> Self {
        Self {
            output: WriterBuilder::new()
                .quote_style(csv::QuoteStyle::NonNumeric)
                .from_writer(output),
        }
    }
}

#[derive(Serialize)]
struct OutputTransaction<'a> {
    #[serde(rename = "TX_ID")]
    id: u64,

    #[serde(rename = "TX_TYPE")]
    tx_type: &'a str,

    #[serde(rename = "FROM_USER_ID")]
    from_user_id: u64,

    #[serde(rename = "TO_USER_ID")]
    to_user_id: u64,

    #[serde(rename = "AMOUNT")]
    amount: u64,

    #[serde(rename = "TIMESTAMP")]
    timestamp: u64,

    #[serde(rename = "STATUS")]
    tx_status: &'a str,

    #[serde(rename = "DESCRIPTION")]
    description: &'a str,
}

impl<W: Write> TransactionWrite for CsvTransactionWriter<W> {
    fn write_next(&mut self, transaction: &Transaction) -> Result<()> {
        let output_transaction = OutputTransaction {
            id: transaction.id(),
            tx_type: transaction_type_to_raw(transaction.tx_type()),
            from_user_id: transaction.from_user_id(),
            to_user_id: transaction.to_user_id(),
            amount: transaction.amount(),
            timestamp: transaction.timestamp(),
            tx_status: transaction_status_to_raw(transaction.tx_status()),
            description: transaction.description(),
        };

        return self
            .output
            .serialize(output_transaction)
            .map_err(|e| Error::from(e));
    }

    fn flush(&mut self) -> Result<()> {
        self.output.flush()
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn trn1() -> Transaction {
        Transaction::new(
            1001,
            TransactionType::Deposit,
            TransactionStatus::Success,
            1672531200000,
            0,
            501,
            50000,
            "Initial account funding".to_string(),
        )
    }

    fn trn2() -> Transaction {
        Transaction::new(
            1002,
            TransactionType::Transfer,
            TransactionStatus::Failure,
            1672534800000,
            501,
            502,
            15000,
            "Payment for services, invoice #123".to_string(),
        )
    }

    fn trn3() -> Transaction {
        Transaction::new(
            1003,
            TransactionType::Withdrawal,
            TransactionStatus::Pending,
            1672538400000,
            502,
            0,
            1000,
            "ATM withdrawal".to_string(),
        )
    }

    #[test]
    fn test_read() {
        let source_text = String::from(
            "TX_ID,TX_TYPE,FROM_USER_ID,TO_USER_ID,AMOUNT,TIMESTAMP,STATUS,DESCRIPTION\n\
            1001,DEPOSIT,0,501,50000,1672531200000,SUCCESS,\"Initial account funding\"\n\
            1002,TRANSFER,501,502,15000,1672534800000,FAILURE,\"Payment for services, invoice #123\"\n\
            1003,WITHDRAWAL,502,0,1000,1672538400000,PENDING,\"ATM withdrawal\"\n",
        );

        let input = Cursor::new(source_text);
        let mut reader = CsvTransactionReader::new(input);

        assert_eq!(reader.read_next().unwrap(), Some(trn1()));
        assert_eq!(reader.read_next().unwrap(), Some(trn2()));
        assert_eq!(reader.read_next().unwrap(), Some(trn3()));
        assert!(reader.read_next().unwrap().is_none());
    }

    #[test]
    fn test_write_read() {
        let mut buffer: Vec<u8> = Vec::new();

        {
            let mut writer = CsvTransactionWriter::new(Cursor::new(&mut buffer));

            assert!(writer.write_next(&trn1()).is_ok());
            assert!(writer.write_next(&trn2()).is_ok());
            assert!(writer.write_next(&trn3()).is_ok());
            assert!(writer.flush().is_ok());
        }

        {
            let mut reader = CsvTransactionReader::new(Cursor::new(&buffer));

            assert_eq!(reader.read_next().unwrap(), Some(trn1()));
            assert_eq!(reader.read_next().unwrap(), Some(trn2()));
            assert_eq!(reader.read_next().unwrap(), Some(trn3()));
            assert!(reader.read_next().unwrap().is_none());
        }
    }
}
