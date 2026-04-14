use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

use chrono::TimeZone;
use clap::Parser;

#[derive(Parser)]
struct Cli {
    #[clap(
        short,
        long,
        help = "Path to the CSV data file to import (Default is stdin)"
    )]
    csv_file: Option<PathBuf>,

    #[clap(
        short,
        long,
        default_value = "Date",
        help = "Name of the date field in the CSV file"
    )]
    date_field: String,

    #[clap(
        long,
        default_value = "%Y-%m-%d",
        help = "Date format used in the CSV file (default is YYYY-MM-DD)"
    )]
    date_format: String,

    #[clap(
        short,
        long,
        help = "Delimiter used in the CSV file (default is comma)"
    )]
    sep: Option<char>,

    #[clap(
        short,
        long,
        help = "Names of the compteur fields to import from the CSV file"
    )]
    fields: Vec<String>,

    #[clap(
        short,
        long,
        help = "Path to the Line Protocol output file (default is stdout)"
    )]
    output_file: Option<PathBuf>,

    #[clap(long, help = "Optional factor to apply to compteur values (e.g. 1000 to convert from kWh to Wh)")]
    factor: Option<f64>,
}

fn main() {
    let cli = Cli::parse();

    eprintln!(
        "Importing data from file: {}",
        cli.csv_file
            .as_ref()
            .map_or("-".to_string(), |p| p.display().to_string())
    );
    eprintln!("Date field: {}", cli.date_field);
    eprintln!("Date format: {}", cli.date_format);
    if let Some(sep) = cli.sep {
        eprintln!("CSV delimiter: {}", sep);
    }
    eprintln!("Fields to import: {:?}", cli.fields);

    match cli.csv_file.as_ref() {
        Some(path) => {
            let reader = csv::ReaderBuilder::new()
                .delimiter(cli.sep.unwrap_or(',') as u8)
                .from_path(path)
                .expect("Failed to open CSV file");
            process_csv(reader, &cli);
        }
        None => {
            let reader = csv::ReaderBuilder::new()
                .delimiter(cli.sep.unwrap_or(',') as u8)
                .from_reader(io::stdin());
            process_csv(reader, &cli);
        }
    }
}

fn process_csv<R: io::Read>(reader: csv::Reader<R>, cli: &Cli) {
    match cli.output_file.as_ref() {
        Some(path) => {
            let output = fs::File::create(path).expect("Failed to create output file");
            process_csv_to_output(reader, cli, output);
        }
        None => {
            let output = io::stdout();
            process_csv_to_output(reader, cli, output);
        }
    }
}

fn process_csv_to_output<R: io::Read, W: io::Write>(
    mut reader: csv::Reader<R>,
    cli: &Cli,
    output: W,
) {
    let headers = reader
        .headers()
        .expect("Failed to read CSV headers")
        .clone();
    let date_field_index = headers
        .iter()
        .position(|h| h == cli.date_field)
        .expect("Date field not found in CSV headers");
    let field_indices: Vec<usize> = cli
        .fields
        .iter()
        .map(|f| {
            headers
                .iter()
                .position(|h| h == f)
                .unwrap_or_else(|| panic!("Field '{}' not found in CSV headers", f))
        })
        .collect();

    let mut buf = io::BufWriter::new(output);
    let records = reader.into_records();

    for record in records {
        let record = record.expect("Failed to read CSV record");

        let date = chrono::NaiveDate::parse_from_str(&record[date_field_index], &cli.date_format)
            .expect("Failed to parse date")
            .and_hms_opt(0, 0, 0)
            .expect("Failed to set time");
        let date = chrono::Local.from_local_datetime(&date).unwrap();
        let timestamp = date.timestamp();

        write!(buf, "compteurs ").expect("Failed to write table name");

        for (i, field_index) in field_indices.iter().enumerate() {
            let field_name = &headers[*field_index];
            let mut field_value: u64 = record[*field_index]
                .parse()
                .expect(&format!("Failed to parse field '{}' as u64", field_name));
            if let Some(factor) = cli.factor {
                field_value = (field_value as f64 * factor) as u64;
            }

            write!(buf, "{field_name}={field_value}u").expect("Failed to write field");
            if i < field_indices.len() - 1 {
                write!(buf, ",").expect("Failed to write field separator");
            }
        }

        write!(buf, " {timestamp}").expect("Failed to write timestamp");
        writeln!(buf).expect("Failed to write newline");
    }
}
