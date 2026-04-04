# Проект "Транзакции"

Реализует сериализацию и десериализацию банковских транзакций в различные форматы.

## Структура

### Основные компоненты

- **`transaction`** — определение структуры транзакции и перечислений для типа и статуса.
- **`io`** — трейты `TransactionRead` и `TransactionWrite` для чтения/записи транзакций.

### Форматы сериализации

- **`bin_format`** — бинарный формат с фиксированной структурой записи (магическое число, размер, поля).
- **`csv_format`** — CSV-формат с заголовком и кавычками для текстовых полей.
- **`text_format`** — текстовый формат с ключевыми словами и пустыми строками-разделителями.

### CLI-приложения

- **`bin/ypbank_compare`** - сравнивает транзакции из двух входных файлов и возвращает список транзакций, для которых обнаружены различия.
- **`bin/ypbank_converter`** - конвертирует транзакции из входного файла в любой другой поддерживаемый формат.

## Примеры использования

### Сериализация / десериализация

```rust
// Запись транзакций
let mut writer = CsvTransactionWriter::new(output);
writer.write_next(&transaction)?;
writer.flush()?;

// Чтение транзакций
let mut reader = CsvTransactionReader::new(input);
while let Some(tx) = reader.read_next()? {
    process(tx);
}
```

### CLI ypbank_compare

```bash
ypbank_compare --file1 records_example.bin --format1 binary --file2 records_example.csv --format2 csv
```

### CLI ypbank_converter

```bash
ypbank_converter \
  --input <input_file> \
  --input-format <format> \
  --output-format <format> \
  > output_file.txt
```
