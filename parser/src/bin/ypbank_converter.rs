use std::env::Args;
use std::fs::File;
use std::io::BufReader;
use std::io::Result as IoResult;
use std::{env, io};

use parser::io::{TransactionRead, TransactionWrite};

use parser::bin_format::{BinTransactionReader, BinTransactionWriter};
use parser::csv_format::{CsvTransactionReader, CsvTransactionWriter};
use parser::text_format::{TextTransactionReader, TextTransactionWriter};

#[derive(Default)]
struct AppArgs {
    input_file: Option<String>,
    input_format: Option<String>,
    output_format: Option<String>,
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
        "--input" => {
            app_args.input_file = read_arg_val(args, arg_name)?;
            Ok(())
        }
        "--input-format" => {
            app_args.input_format = read_arg_val(args, arg_name)?;
            Ok(())
        }
        "--output-format" => {
            app_args.output_format = read_arg_val(args, arg_name)?;
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

fn build_writer(output_format: &String) -> Result<Box<dyn TransactionWrite>, ()> {
    let output = io::stdout();

    match output_format.as_str() {
        "bin" => Ok(Box::new(BinTransactionWriter::new(output))),
        "txt" => Ok(Box::new(TextTransactionWriter::new(output))),
        "csv" => Ok(Box::new(CsvTransactionWriter::new(output))),
        _ => {
            eprintln!("Unknown output format \"{output_format}\"");
            Err(())
        }
    }
}

fn do_work() -> Result<(), ()> {
    let args = parse_args()?;

    let input_file = args.input_file.ok_or_else(|| {
        eprintln!("Missed --input arg");
    })?;

    let input_format = args.input_format.ok_or_else(|| {
        eprintln!("Missed --input-format arg");
    })?;

    let output_format = args.output_format.ok_or_else(|| {
        eprintln!("Missed --output-format arg");
    })?;

    let input = File::open(input_file).map_err(|e| {
        eprintln!("Failed to open file: {e}");
    })?;

    let reader = build_reader(input, &input_format)?;
    let writer = build_writer(&output_format)?;

    return convert(reader, writer).map_err(|e| {
        eprintln!("{e}");
    });
}

fn convert(
    mut reader: Box<dyn TransactionRead>,
    mut writer: Box<dyn TransactionWrite>,
) -> IoResult<()> {
    while let Some(transaction) = reader.read_next()? {
        writer.write_next(&transaction)?;
    }

    return Ok(());
}

fn main() {
    _ = do_work();
}
