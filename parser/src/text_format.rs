use std::fmt::Display;
use std::io::{BufRead, Error, ErrorKind, Result, Write};
use std::str::FromStr;

use crate::io::{TransactionRead, TransactionWrite};
use crate::transaction::{Transaction, TransactionStatus, TransactionType};

const FIELD_COMMENT: &str = "#";
const FIELD_ID: &str = "TX_ID:";
const FIELD_TYPE: &str = "TX_TYPE:";
const FIELD_FROM_USER_ID: &str = "FROM_USER_ID:";
const FIELD_TO_USER_ID: &str = "TO_USER_ID:";
const FIELD_AMOUNT: &str = "AMOUNT:";
const FIELD_TIMESTAMP: &str = "TIMESTAMP:";
const FIELD_STATUS: &str = "STATUS:";
const FIELD_DESCRIPTION: &str = "DESCRIPTION:";

const TRANSACTION_TYPE_DEPOSIT: &str = "DEPOSIT";
const TRANSACTION_TYPE_TRANSFER: &str = "TRANSFER";
const TRANSACTION_TYPE_WITHDRAWAL: &str = "WITHDRAWAL";

const TRANSACTION_STATUS_SUCCESS: &str = "SUCCESS";
const TRANSACTION_STATUS_FAILURE: &str = "FAILURE";
const TRANSACTION_STATUS_PENDING: &str = "PENDING";

const TRANSACTION_DESCRIPTION_LIMITER: &str = "\"";

fn error_invalid_data(error_message: String) -> Error {
    Error::new(ErrorKind::InvalidData, error_message)
}

fn err_invalid_data<T>(error_message: String) -> Result<T> {
    Err(error_invalid_data(error_message))
}

fn unwrap_field<T>(field: Option<T>, field_title: &str) -> Result<T> {
    field.ok_or(error_invalid_data(format!(
        "Missed {field_title} in transaction record"
    )))
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

fn transaction_description_from_raw(description_raw: &str) -> Result<String> {
    if description_raw.starts_with(TRANSACTION_DESCRIPTION_LIMITER)
        && description_raw.ends_with(TRANSACTION_DESCRIPTION_LIMITER)
    {
        Ok(description_raw[1..description_raw.len() - 1].to_string())
    } else {
        err_invalid_data(
            "Illegal value for transaction description (missed start/final \")".to_string(),
        )
    }
}

struct TransactionFields {
    id: Option<u64>,
    tx_type: Option<TransactionType>,

    from_user_id: Option<u64>,
    to_user_id: Option<u64>,
    amount: Option<u64>,

    timestamp: Option<u64>,
    tx_status: Option<TransactionStatus>,

    description: Option<String>,
}

impl TransactionFields {
    fn empty() -> Self {
        TransactionFields {
            id: Option::None,
            tx_type: Option::None,
            from_user_id: Option::None,
            to_user_id: Option::None,
            amount: Option::None,
            timestamp: Option::None,
            tx_status: Option::None,
            description: Option::None,
        }
    }

    fn is_empty(&self) -> bool {
        self.id.is_none()
            && self.tx_type.is_none()
            && self.from_user_id.is_none()
            && self.to_user_id.is_none()
            && self.amount.is_none()
            && self.timestamp.is_none()
            && self.tx_status.is_none()
            && self.description.is_none()
    }
}

enum TransactionLineReadResult {
    Empty,
    Comment,
    Field,
    Eof,
}

/// Десериализация списка транзакций в текстовом формате
pub struct TextTransactionReader<R: BufRead> {
    input: R,
    input_line_buffer: String,
}

impl<R: BufRead> TextTransactionReader<R> {
    pub fn new(input: R) -> Self {
        Self {
            input: input,
            input_line_buffer: String::new(),
        }
    }

    fn read_transaction_fields(&mut self) -> Result<TransactionFields> {
        let mut fields = TransactionFields::empty();

        loop {
            let line_read_result = self.read_next_field(&mut fields)?;

            match line_read_result {
                TransactionLineReadResult::Empty => {
                    if !fields.is_empty() {
                        break;
                    }
                }
                TransactionLineReadResult::Eof => {
                    break;
                }
                TransactionLineReadResult::Comment | TransactionLineReadResult::Field => {
                    // Do nothing (continue to next loop)
                }
            }
        }

        return Ok(fields);
    }

    fn read_next_field(
        &mut self,
        fields: &mut TransactionFields,
    ) -> Result<TransactionLineReadResult> {
        self.input_line_buffer.clear();

        let bytes_read = self.input.read_line(&mut self.input_line_buffer)?;

        if bytes_read == 0 {
            return Ok(TransactionLineReadResult::Eof);
        }

        if self.input_line_buffer.trim().is_empty() {
            return Ok(TransactionLineReadResult::Empty);
        }

        if self.input_line_buffer.starts_with(FIELD_COMMENT) {
            return Ok(TransactionLineReadResult::Comment);
        }

        let field_parsed = self.try_parse_u64_field(FIELD_ID, "id", &mut fields.id)?
            || self.try_parse_field(FIELD_TYPE, "type", &mut fields.tx_type, |tx_type_raw| {
                transaction_type_from_raw(tx_type_raw)
            })?
            || self.try_parse_u64_field(
                FIELD_FROM_USER_ID,
                "from_user_id",
                &mut fields.from_user_id,
            )?
            || self.try_parse_u64_field(FIELD_TO_USER_ID, "to_user_id", &mut fields.to_user_id)?
            || self.try_parse_u64_field(FIELD_AMOUNT, "amount", &mut fields.amount)?
            || self.try_parse_u64_field(FIELD_TIMESTAMP, "timestamp", &mut fields.timestamp)?
            || self.try_parse_field(
                FIELD_STATUS,
                "status",
                &mut fields.tx_status,
                |tx_status_raw| transaction_status_from_raw(tx_status_raw),
            )?
            || self.try_parse_field(
                FIELD_DESCRIPTION,
                "description",
                &mut fields.description,
                |description_raw| transaction_description_from_raw(description_raw),
            )?;

        if !field_parsed {
            return err_invalid_data(format!(
                "Failed to parse field at line: \"{}\"",
                &self.input_line_buffer
            ));
        }

        return Ok(TransactionLineReadResult::Field);
    }

    fn try_parse_field<V>(
        &mut self,
        field_prefix: &str,
        field_title: &str,
        field: &mut Option<V>,
        parse: impl Fn(&str) -> Result<V>,
    ) -> Result<bool> {
        if !self.input_line_buffer.starts_with(field_prefix) {
            return Ok(false);
        }

        if let Option::Some(_) = field {
            return err_invalid_data(format!("Found duplicated entry for {field_title}"));
        }

        let field_value_str = self.input_line_buffer.as_str()[field_prefix.len()..].trim();
        let field_value = parse(field_value_str)?;

        *field = Option::Some(field_value);

        return Ok(true);
    }

    fn parse_u64_field_value(field_title: &str, field_value: &str) -> Result<u64> {
        u64::from_str(field_value)
            .map_err(|e| error_invalid_data(format!("Failed to parse {field_title} value: {e}")))
    }

    fn try_parse_u64_field(
        &mut self,
        field_prefix: &str,
        field_title: &str,
        field: &mut Option<u64>,
    ) -> Result<bool> {
        self.try_parse_field(field_prefix, field_title, field, |field_value| {
            Self::parse_u64_field_value(field_title, field_value)
        })
    }
}

impl<R: BufRead> TransactionRead for TextTransactionReader<R> {
    fn read_next(&mut self) -> Result<Option<Transaction>> {
        let mut fields = self.read_transaction_fields()?;

        if fields.is_empty() {
            return Ok(Option::None);
        }

        let id = unwrap_field(fields.id, "id")?;
        let tx_type = unwrap_field(fields.tx_type, "type")?;

        if fields.from_user_id.is_none() && tx_type == TransactionType::Deposit {
            fields.from_user_id = Some(0);
        }

        if fields.to_user_id.is_none() && tx_type == TransactionType::Withdrawal {
            fields.to_user_id = Some(0);
        }

        let from_user_id = unwrap_field(fields.from_user_id, "from_user_id")?;
        let to_user_id = unwrap_field(fields.to_user_id, "to_user_id")?;
        let amount = unwrap_field(fields.amount, "amount")?;

        let timestamp = unwrap_field(fields.timestamp, "timestamp")?;
        let tx_status = unwrap_field(fields.tx_status, "status")?;

        if fields.description.is_none() {
            fields.description = Some(String::new());
        }

        let description = unwrap_field(fields.description, "description")?;

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

/// Сериализация списка транзакций в текстовый формат
pub struct TextTransactionWriter<W: Write> {
    output: W,
}

impl<W: Write> TextTransactionWriter<W> {
    pub fn new(output: W) -> Self {
        Self { output: output }
    }

    fn write_field<V: Display + Copy>(&mut self, field: &str, value: V) -> Result<()> {
        writeln!(self.output, "{field} {value}")
    }
}

impl<W: Write> TransactionWrite for TextTransactionWriter<W> {
    fn write_next(&mut self, transaction: &Transaction) -> Result<()> {
        writeln!(self.output, "")?;
        self.write_field(FIELD_ID, transaction.id())?;
        self.write_field(FIELD_TYPE, transaction_type_to_raw(transaction.tx_type()))?;
        self.write_field(FIELD_FROM_USER_ID, transaction.from_user_id())?;
        self.write_field(FIELD_TO_USER_ID, transaction.to_user_id())?;
        self.write_field(FIELD_AMOUNT, transaction.amount())?;
        self.write_field(FIELD_TIMESTAMP, transaction.timestamp())?;
        self.write_field(
            FIELD_STATUS,
            transaction_status_to_raw(transaction.tx_status()),
        )?;
        writeln!(
            self.output,
            "{} \"{}\"",
            FIELD_DESCRIPTION,
            transaction.description()
        )?;

        return Ok(());
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
            1234567890123456,
            TransactionType::Deposit,
            TransactionStatus::Success,
            1633036800000,
            0,
            9876543210987654,
            10000,
            "Terminal deposit".to_string(),
        )
    }

    fn trn2() -> Transaction {
        Transaction::new(
            2312321321321321,
            TransactionType::Transfer,
            TransactionStatus::Failure,
            1633056800000,
            1231231231231231,
            9876543210987654,
            1000,
            "User transfer".to_string(),
        )
    }

    fn trn3() -> Transaction {
        Transaction::new(
            3213213213213213,
            TransactionType::Withdrawal,
            TransactionStatus::Success,
            1633066800000,
            9876543210987654,
            0,
            100,
            "User withdrawal".to_string(),
        )
    }

    #[test]
    fn test_read() {
        let source_text = String::from(
            "# Record 1 (Deposit)\n\
            TX_ID: 1234567890123456\n\
            TX_TYPE: DEPOSIT\n\
            FROM_USER_ID: 0\n\
            TO_USER_ID: 9876543210987654\n\
            AMOUNT: 10000\n\
            TIMESTAMP: 1633036800000\n\
            STATUS: SUCCESS\n\
            DESCRIPTION: \"Terminal deposit\"\n\
            \n\
            # Record 2 (Transfer)\n\
            TX_ID: 2312321321321321\n\
            TIMESTAMP: 1633056800000\n\
            STATUS: FAILURE\n\
            TX_TYPE: TRANSFER\n\
            FROM_USER_ID: 1231231231231231\n\
            TO_USER_ID: 9876543210987654\n\
            AMOUNT: 1000\n\
            DESCRIPTION: \"User transfer\"\n\
            \n\
            # Record 3 (Withdrawal)\n\
            TX_ID: 3213213213213213\n\
            AMOUNT: 100\n\
            TX_TYPE: WITHDRAWAL\n\
            FROM_USER_ID: 9876543210987654\n\
            TO_USER_ID: 0\n\
            TIMESTAMP: 1633066800000\n\
            STATUS: SUCCESS\n\
            DESCRIPTION: \"User withdrawal\"\n",
        );

        let input = Cursor::new(source_text);
        let mut reader = TextTransactionReader::new(input);

        assert_eq!(reader.read_next().unwrap(), Some(trn1()));
        assert_eq!(reader.read_next().unwrap(), Some(trn2()));
        assert_eq!(reader.read_next().unwrap(), Some(trn3()));
        assert!(reader.read_next().unwrap().is_none());
    }

    #[test]
    fn test_write_read() {
        let mut buffer: Vec<u8> = Vec::new();

        let mut writer = TextTransactionWriter::new(Cursor::new(&mut buffer));

        assert!(writer.write_next(&trn1()).is_ok());
        assert!(writer.write_next(&trn2()).is_ok());
        assert!(writer.write_next(&trn3()).is_ok());
        assert!(writer.flush().is_ok());

        let mut reader = TextTransactionReader::new(Cursor::new(&buffer));

        assert_eq!(reader.read_next().unwrap(), Some(trn1()));
        assert_eq!(reader.read_next().unwrap(), Some(trn2()));
        assert_eq!(reader.read_next().unwrap(), Some(trn3()));
        assert!(reader.read_next().unwrap().is_none());
    }
}
