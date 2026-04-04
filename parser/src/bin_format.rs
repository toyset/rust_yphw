use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::io::{BufRead, Error, ErrorKind, Result, Write};
use std::mem::size_of;

use crate::io::{TransactionRead, TransactionWrite};
use crate::transaction::{Transaction, TransactionStatus, TransactionType};

type RecordHeaderMagicType = [u8; 4];

static RECORD_HEADER_MAGIC: RecordHeaderMagicType = [0x59, 0x50, 0x42, 0x4E];
static FIXED_RECORD_SIZE: usize = 5 * size_of::<u64>()  // id, from_user_id, to_user_id, amount, timestamp
    + 2 * size_of::<u8>()   // type, status
    + size_of::<u32>(); // descr_len

const TRANSACTION_TYPE_DEPOSIT: u8 = 0;
const TRANSACTION_TYPE_TRANSFER: u8 = 1;
const TRANSACTION_TYPE_WITHDRAWAL: u8 = 2;

const TRANSACTION_STATUS_SUCCESS: u8 = 0;
const TRANSACTION_STATUS_FAILURE: u8 = 1;
const TRANSACTION_STATUS_PENDING: u8 = 2;

fn transaction_type_from_raw(tx_type_raw: u8) -> Result<TransactionType> {
    match tx_type_raw {
        TRANSACTION_TYPE_DEPOSIT => Ok(TransactionType::Deposit),
        TRANSACTION_TYPE_TRANSFER => Ok(TransactionType::Transfer),
        TRANSACTION_TYPE_WITHDRAWAL => Ok(TransactionType::Withdrawal),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Illegal value for transaction type ({tx_type_raw})"),
        )),
    }
}

fn transaction_type_to_raw(tx_type: TransactionType) -> u8 {
    match tx_type {
        TransactionType::Deposit => TRANSACTION_TYPE_DEPOSIT,
        TransactionType::Transfer => TRANSACTION_TYPE_TRANSFER,
        TransactionType::Withdrawal => TRANSACTION_TYPE_WITHDRAWAL,
    }
}

fn transaction_status_from_raw(tx_status_raw: u8) -> Result<TransactionStatus> {
    match tx_status_raw {
        TRANSACTION_STATUS_SUCCESS => Ok(TransactionStatus::Success),
        TRANSACTION_STATUS_FAILURE => Ok(TransactionStatus::Failure),
        TRANSACTION_STATUS_PENDING => Ok(TransactionStatus::Pending),
        _ => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Illegal value for transaction status ({tx_status_raw})"),
        )),
    }
}

fn transaction_status_to_raw(tx_status: TransactionStatus) -> u8 {
    match tx_status {
        TransactionStatus::Success => TRANSACTION_STATUS_SUCCESS,
        TransactionStatus::Failure => TRANSACTION_STATUS_FAILURE,
        TransactionStatus::Pending => TRANSACTION_STATUS_PENDING,
    }
}

/// Десериализация списка транзакций в бинарном формате
pub struct BinTransactionReader<R: BufRead> {
    input: R,
}

impl<R: BufRead> BinTransactionReader<R> {
    pub fn new(input: R) -> Self {
        Self { input: input }
    }

    fn read_transaction_description(&mut self, record_size_from_header: usize) -> Result<String> {
        let descr_len = self.input.read_u32::<BigEndian>()? as usize;

        let actual_record_size = FIXED_RECORD_SIZE + descr_len;

        if actual_record_size != record_size_from_header {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Actual record size ({actual_record_size}) differs from size, declared in record header ({record_size_from_header})"
                ),
            ));
        }

        if descr_len == 0 {
            return Ok(String::new());
        }

        let mut buffer = vec![0u8; descr_len];

        self.input.read_exact(&mut buffer)?;

        return String::from_utf8(buffer).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Invalid UTF-8 sequence for description field ({e})"),
            )
        });
    }

    fn read_record_body(&mut self, record_size_from_header: usize) -> Result<Transaction> {
        if record_size_from_header < FIXED_RECORD_SIZE {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "Record size, declared in record header ({record_size_from_header}), is less than minimal record size ({FIXED_RECORD_SIZE})"
                ),
            ));
        }

        let id = self.input.read_u64::<BigEndian>()?;
        let tx_type = transaction_type_from_raw(self.input.read_u8()?)?;

        let from_user_id = self.input.read_u64::<BigEndian>()?;
        let to_user_id = self.input.read_u64::<BigEndian>()?;
        let amount = self.input.read_u64::<BigEndian>()?;

        let timestamp = self.input.read_u64::<BigEndian>()?;
        let tx_status = transaction_status_from_raw(self.input.read_u8()?)?;

        let description = self.read_transaction_description(record_size_from_header)?;

        let transaction = Transaction::new(
            id,
            tx_type,
            tx_status,
            timestamp,
            from_user_id,
            to_user_id,
            amount,
            description,
        );

        return Ok(transaction);
    }
}

impl<R: BufRead> TransactionRead for BinTransactionReader<R> {
    fn read_next(&mut self) -> Result<Option<Transaction>> {
        let mut record_magic: RecordHeaderMagicType = [0; RECORD_HEADER_MAGIC.len()];

        if let Err(error) = self.input.read_exact(&mut record_magic[..1]) {
            return match error.kind() {
                ErrorKind::UnexpectedEof => Ok(None), // Actually, expected EOF
                _ => Err(error),
            };
        }

        self.input.read_exact(&mut record_magic[1..])?;

        if record_magic != RECORD_HEADER_MAGIC {
            return Err(Error::new(
                ErrorKind::InvalidData,
                "Incorrect record header magic",
            ));
        }

        let record_size = self.input.read_u32::<BigEndian>()? as usize;

        return self
            .read_record_body(record_size)
            .map(|transaction| Some(transaction));
    }
}

/// Сериализация списка транзакций в бинарный формат
pub struct BinTransactionWriter<W: Write> {
    output: W,
}

impl<W: Write> BinTransactionWriter<W> {
    pub fn new(output: W) -> Self {
        Self { output: output }
    }

    fn write_record_header(&mut self, transaction: &Transaction) -> Result<()> {
        self.output.write_all(&RECORD_HEADER_MAGIC)?;
        self.output
            .write_u32::<BigEndian>((FIXED_RECORD_SIZE + transaction.description().len()) as u32)?;

        return Ok(());
    }

    fn write_transaction_description(&mut self, transaction_description: &String) -> Result<()> {
        self.output
            .write_u32::<BigEndian>(transaction_description.len() as u32)?;
        self.output.write_all(transaction_description.as_bytes())?;

        return Ok(());
    }

    fn write_record_body(&mut self, transaction: &Transaction) -> Result<()> {
        self.output.write_u64::<BigEndian>(transaction.id())?;
        self.output
            .write_u8(transaction_type_to_raw(transaction.tx_type()))?;

        self.output
            .write_u64::<BigEndian>(transaction.from_user_id())?;
        self.output
            .write_u64::<BigEndian>(transaction.to_user_id())?;
        self.output.write_u64::<BigEndian>(transaction.amount())?;

        self.output
            .write_u64::<BigEndian>(transaction.timestamp())?;
        self.output
            .write_u8(transaction_status_to_raw(transaction.tx_status()))?;

        self.write_transaction_description(transaction.description())?;

        return Ok(());
    }
}

impl<W: Write> TransactionWrite for BinTransactionWriter<W> {
    fn write_next(&mut self, transaction: &Transaction) -> Result<()> {
        self.write_record_header(transaction)?;

        return self.write_record_body(transaction);
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
    fn test_write_read() {
        let mut buffer: Vec<u8> = Vec::new();

        {
            let mut writer = BinTransactionWriter::new(Cursor::new(&mut buffer));

            assert!(writer.write_next(&trn1()).is_ok());
            assert!(writer.write_next(&trn2()).is_ok());
            assert!(writer.write_next(&trn3()).is_ok());
            assert!(writer.flush().is_ok());
        }

        {
            let mut reader = BinTransactionReader::new(Cursor::new(&buffer));

            assert_eq!(reader.read_next().unwrap(), Some(trn1()));
            assert_eq!(reader.read_next().unwrap(), Some(trn2()));
            assert_eq!(reader.read_next().unwrap(), Some(trn3()));
            assert!(reader.read_next().unwrap().is_none());
        }
    }
}
