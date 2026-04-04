use std::collections::BTreeMap;
use std::env;
use std::env::Args;
use std::fs::File;
use std::io::BufReader;
use std::io::Result as IoResult;

use parser::io::TransactionRead;
use parser::transaction::Transaction;

use parser::bin_format::BinTransactionReader;
use parser::csv_format::CsvTransactionReader;
use parser::text_format::TextTransactionReader;

type TransactionMap = BTreeMap<u64, Transaction>;

#[derive(Default)]
struct AppArgs {
    file1: Option<String>,
    format1: Option<String>,
    file2: Option<String>,
    format2: Option<String>,
}

fn parse_args() -> Result<AppArgs, ()> {
    let mut args = env::args();
    let mut app_args = AppArgs::default();

    args.next();

    while let Some(arg) = args.next() {
        parse_next_arg(&mut args, arg, &mut app_args)?;
    }

    return Ok(app_args);
}

fn parse_next_arg(args: &mut Args, arg_name: String, app_args: &mut AppArgs) -> Result<(), ()> {
    if !arg_name.starts_with("-") {
        return Ok(());
    }

    match arg_name.as_str() {
        "--file1" => {
            app_args.file1 = read_arg_val(args, arg_name)?;
            Ok(())
        }
        "--format1" => {
            app_args.format1 = read_arg_val(args, arg_name)?;
            Ok(())
        }
        "--file2" => {
            app_args.file2 = read_arg_val(args, arg_name)?;
            Ok(())
        }
        "--format2" => {
            app_args.format2 = read_arg_val(args, arg_name)?;
            Ok(())
        }
        _ => {
            eprintln!("Unknown arg {arg_name}");
            Err(())
        }
    }
}

fn read_arg_val(args: &mut Args, arg_name: String) -> Result<Option<String>, ()> {
    let arg_val = args.next();

    if arg_val.is_none() {
        eprintln!("No value for arg \"{arg_name}\"");
        return Err(());
    }

    return Ok(arg_val);
}

fn build_reader(input_file: File, input_format: &String) -> Result<Box<dyn TransactionRead>, ()> {
    let input = BufReader::new(input_file);

    match input_format.as_str() {
        "bin" => Ok(Box::new(BinTransactionReader::new(input))),
        "txt" => Ok(Box::new(TextTransactionReader::new(input))),
        "csv" => Ok(Box::new(CsvTransactionReader::new(input))),
        _ => {
            eprintln!("Unknown input format \"{input_format}\"");
            Err(())
        }
    }
}

fn read_file(file: String, format: String, source_name: &str) -> Result<TransactionMap, ()> {
    let input = File::open(file).map_err(|e| {
        eprintln!("Failed to open {source_name}: {e}");
    })?;

    return read_transactions(build_reader(input, &format)?, source_name).map_err(|e| {
        eprintln!("Failed to read {source_name}: {e}");
    });
}

fn read_transactions(
    mut reader: Box<dyn TransactionRead>,
    source_name: &str,
) -> IoResult<TransactionMap> {
    let mut transactions_by_id = TransactionMap::new();

    while let Some(transaction) = reader.read_next()? {
        transactions_by_id
            .entry(transaction.id())
            .and_modify(|existing_transaction| {
                if *existing_transaction != transaction {
                    eprintln!(
                        "Found duplicated transaction {} in {}",
                        transaction.id(),
                        source_name
                    );
                }
            })
            .or_insert(transaction);
    }

    return Ok(transactions_by_id);
}

fn compare_transactions(
    transactions1_by_id: &TransactionMap,
    transactions2_by_id: &TransactionMap,
) {
    let mut different_count: u64 = 0;
    let mut missed_in_file2_count: u64 = 0;
    let mut missed_in_file1_count: u64 = 0;

    for transaction1 in transactions1_by_id.values() {
        if let Some(transaction2) = transactions2_by_id.get(&transaction1.id()) {
            if transaction1 != transaction2 {
                different_count += 1;
                println!(
                    "Transaction {} records are different in file1 and file2",
                    transaction1.id()
                );
            }
        } else {
            missed_in_file2_count += 1;
            println!(
                "Transaction {} from file1 not exists in file2",
                transaction1.id()
            );
        }
    }

    for transaction2 in transactions2_by_id.values() {
        if !transactions1_by_id.contains_key(&transaction2.id()) {
            missed_in_file1_count += 1;
            println!(
                "Transaction {} from file2 not exists in file1",
                transaction2.id()
            );
        }
    }

    if different_count != 0 {
        println!("Found {different_count} different transaction records in file1 and file2");
    }

    if missed_in_file2_count != 0 {
        println!("{missed_in_file2_count} file1 tansactions missed in file2");
    }

    if missed_in_file1_count != 0 {
        println!("{missed_in_file1_count} file2 tansactions missed in file1");
    }

    if different_count == 0 && missed_in_file2_count == 0 && missed_in_file1_count == 0 {
        println!("The transaction records in file1 and file2 are identical.");
    }
}

fn do_work() -> Result<(), ()> {
    let args = parse_args()?;

    let file1 = args.file1.ok_or_else(|| {
        eprintln!("Missed --file1 arg");
    })?;

    let format1 = args.format1.ok_or_else(|| {
        eprintln!("Missed --format1 arg");
    })?;

    let file2 = args.file2.ok_or_else(|| {
        eprintln!("Missed --file2 arg");
    })?;

    let format2 = args.format2.ok_or_else(|| {
        eprintln!("Missed --format2 arg");
    })?;

    let transactions1_by_id = read_file(file1, format1, "file1")?;
    let transactions2_by_id = read_file(file2, format2, "file2")?;

    compare_transactions(&transactions1_by_id, &transactions2_by_id);

    return Ok(());
}

fn main() {
    _ = do_work();
}
