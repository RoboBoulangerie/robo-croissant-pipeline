use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    error::Error,
    fs::File,
    io::{self, BufRead, BufReader, Seek, SeekFrom},
    path::Path,
};

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use flate2::read::MultiGzDecoder;
use noodles::{bam, bcf, sam, vcf};
use polars::{io::avro::AvroReader, io::ipc::IpcReader, io::parquet::read::ParquetReader, prelude::*};
use serde::Serialize;

const SAM_COLUMNS: &[(&str, &str)] = &[
    ("QNAME", "TEXT"),
    ("FLAG", "INTEGER"),
    ("RNAME", "TEXT"),
    ("POS", "INTEGER"),
    ("MAPQ", "INTEGER"),
    ("CIGAR", "TEXT"),
    ("RNEXT", "TEXT"),
    ("PNEXT", "INTEGER"),
    ("TLEN", "INTEGER"),
    ("SEQ", "TEXT"),
    ("QUAL", "TEXT"),
];
const VCF_COLUMNS: &[(&str, &str)] =
    &[("#CHROM", "TEXT"), ("POS", "INTEGER"), ("ID", "TEXT"), ("REF", "TEXT"), ("ALT", "TEXT"), ("QUAL", "FLOAT"), ("FILTER", "TEXT"), ("INFO", "TEXT")];

#[derive(Debug, PartialEq, Serialize)]
struct Column {
    name: String,
    datatype: String,
}

impl Column {
    fn new(name: impl Into<String>, datatype: impl Into<String>) -> Self {
        Self { name: name.into(), datatype: datatype.into() }
    }
}

#[derive(Serialize)]
struct ParsedSchema {
    columns: Vec<Column>,
}

#[derive(Debug, Serialize)]
struct JsonlField {
    name: String,
    optional: bool,
    datatype: String,
}

#[derive(Debug, Serialize)]
struct JsonlTable {
    name: String,
    fields: Vec<JsonlField>,
}

#[derive(Debug, Serialize)]
struct JsonlSchema {
    tables: [JsonlTable; 1],
}

fn comment_body(line: &str) -> Option<&str> {
    let line = line.trim_start();
    ["//", "--", "#", ";"].into_iter().find_map(|prefix| line.strip_prefix(prefix)).map(str::trim)
}

fn schema_declaration(comment: &str) -> Option<&str> {
    let (marker, value) = comment.split_once(':').unwrap_or((comment, ""));
    ["fields", "columns", "schema"].iter().any(|candidate| marker.trim().eq_ignore_ascii_case(candidate)).then(|| value.trim())
}

fn inspect_preamble(path: &Path) -> io::Result<(Option<String>, u64, Option<String>)> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut declared_header = None;
    let mut awaiting_declared_header = false;
    let mut offset = 0;

    loop {
        let line_offset = offset;
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        offset += bytes_read as u64;

        if bytes_read == 0 {
            return Ok((declared_header, offset, None));
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Some(comment) = comment_body(&line) {
            if let Some(value) = schema_declaration(comment) {
                awaiting_declared_header = value.is_empty();
                if !value.is_empty() {
                    declared_header = Some(value.to_owned());
                }
            } else if awaiting_declared_header && !comment.is_empty() {
                declared_header = Some(comment.to_owned());
                awaiting_declared_header = false;
            }
            continue;
        }

        return Ok((declared_header, line_offset, Some(line)));
    }
}

fn read_delimited_rows(path: &Path, delimiter: char, offset: u64, limit: usize) -> io::Result<Vec<csv::StringRecord>> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;

    csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter as u8)
        .from_reader(file)
        .records()
        .take(limit)
        .map(|record| record.map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error)))
        .collect()
}

fn likely_has_header_rows(rows: &[csv::StringRecord]) -> bool {
    if rows.len() < 2 || rows[0].len() != rows[1].len() {
        return false;
    }

    fn looks_like_data(value: &str) -> bool {
        value.is_empty() || value.parse::<f64>().is_ok() || matches!(value.to_ascii_lowercase().as_str(), "true" | "false" | "null" | "na" | "n/a")
    }

    let first_row_values = rows[0].iter().filter(|value| looks_like_data(value)).count();
    let second_row_values = rows[1].iter().filter(|value| looks_like_data(value)).count();

    first_row_values < second_row_values
}

pub fn likely_has_header(path: impl AsRef<Path>, delimiter: char) -> io::Result<bool> {
    let path = path.as_ref();
    let (_, offset, _) = inspect_preamble(path)?;
    Ok(likely_has_header_rows(&read_delimited_rows(path, delimiter, offset, 2)?))
}

fn infer_value_datatype(value: &str) -> Option<&'static str> {
    let value = value.trim().trim_matches('"');
    let lowercase = value.to_ascii_lowercase();

    if matches!(lowercase.as_str(), "" | "null" | "na" | "n/a") {
        None
    } else if value.parse::<i64>().is_ok() {
        Some("INTEGER")
    } else if value.parse::<f64>().is_ok() {
        Some("FLOAT")
    } else if matches!(lowercase.as_str(), "true" | "false") {
        Some("BOOLEAN")
    } else if DateTime::parse_from_rfc3339(value).is_ok()
        || NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").is_ok()
        || NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").is_ok()
    {
        Some("DATETIME")
    } else if NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok() {
        Some("DATE")
    } else {
        Some("TEXT")
    }
}

fn merge_datatypes(current: Option<&'static str>, next: Option<&'static str>) -> Option<&'static str> {
    match (current, next) {
        (datatype, None) | (None, datatype) => datatype,
        (Some(left), Some(right)) if left == right => Some(left),
        (Some("INTEGER"), Some("FLOAT")) | (Some("FLOAT"), Some("INTEGER")) => Some("FLOAT"),
        (Some("DATE"), Some("DATETIME")) | (Some("DATETIME"), Some("DATE")) => Some("DATETIME"),
        _ => Some("TEXT"),
    }
}

fn parse_delimited_headers(path: &Path, delimiter: Option<char>) -> Result<Vec<Column>, Box<dyn Error>> {
    let (declared_header, offset, first_line) = inspect_preamble(path)?;
    let first_line = first_line.ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "file contains no data rows"))?;
    // ponytail: text files only sniff comma vs tab; add broader detection if inputs require it.
    let delimiter = delimiter.unwrap_or_else(|| if first_line.contains('\t') { '\t' } else { ',' });
    let rows = read_delimited_rows(path, delimiter, offset, 6)?;
    let first_row = rows.first().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "file contains no data rows"))?;
    let has_header = likely_has_header_rows(&rows);
    let declared_names = if let Some(header) = declared_header {
        let mut reader = csv::ReaderBuilder::new().has_headers(false).delimiter(delimiter as u8).from_reader(header.as_bytes());
        reader.records().next().transpose()?.map(|record| record.iter().map(str::to_owned).collect::<Vec<_>>())
    } else {
        None
    };
    let column_count = declared_names.as_ref().map_or_else(|| first_row.len(), Vec::len);
    let names = if let Some(names) = declared_names {
        names
    } else if has_header {
        first_row.iter().map(str::to_owned).collect::<Vec<_>>()
    } else {
        (1..=column_count).map(|index| format!("column_{index}")).collect()
    };
    let mut datatypes = vec![None; column_count];

    for row in rows.iter().skip(usize::from(has_header)).take(5) {
        for (index, value) in row.iter().take(column_count).enumerate() {
            datatypes[index] = merge_datatypes(datatypes[index], infer_value_datatype(value));
        }
    }

    Ok(names.into_iter().zip(datatypes).map(|(name, datatype)| Column::new(name, datatype.unwrap_or("TEXT"))).collect())
}

fn fixed_columns(columns: &[(&str, &str)]) -> Vec<Column> {
    columns.iter().map(|(name, datatype)| Column::new(*name, *datatype)).collect()
}

fn vcf_columns<'a>(samples: impl ExactSizeIterator<Item = &'a String>) -> Vec<Column> {
    let mut columns = fixed_columns(VCF_COLUMNS);

    if samples.len() > 0 {
        columns.push(Column::new("FORMAT", "TEXT"));
    }

    columns.extend(samples.map(|sample| Column::new(sample, "TEXT")));
    columns
}

fn polars_datatype(datatype: &DataType) -> String {
    match datatype {
        DataType::Boolean => "BOOLEAN".into(),
        DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::UInt128
        | DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::Int128 => "INTEGER".into(),
        DataType::Float16 | DataType::Float32 | DataType::Float64 => "FLOAT".into(),
        DataType::String => "TEXT".into(),
        DataType::Binary | DataType::BinaryOffset => "BINARY".into(),
        DataType::Date => "DATE".into(),
        DataType::Datetime(_, _) => "DATETIME".into(),
        DataType::Duration(_) => "DURATION".into(),
        DataType::Time => "TIME".into(),
        DataType::Null => "NULL".into(),
        datatype => datatype.pretty_format().to_ascii_uppercase(),
    }
}

fn polars_columns(schema: &Schema) -> Vec<Column> {
    schema.iter().map(|(name, datatype)| Column::new(name.as_str(), polars_datatype(datatype))).collect()
}

fn arrow_columns(schema: &ArrowSchema) -> Vec<Column> {
    schema.iter().map(|(name, field)| Column::new(name.as_str(), polars_datatype(&DataType::from_arrow_field(field)))).collect()
}

fn json_datatype(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

fn parse_jsonl_schema(path: &Path) -> Result<JsonlSchema, Box<dyn Error>> {
    let file = File::open(path)?;
    let input: Box<dyn io::Read> =
        if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("gz")) { Box::new(MultiGzDecoder::new(file)) } else { Box::new(file) };
    let mut row_count = 0usize;
    let mut fields = BTreeMap::<String, (usize, BTreeSet<&'static str>)>::new();

    for line in BufReader::new(input).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let row = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&line)?;
        row_count += 1;
        for (name, value) in row {
            let (present_count, datatypes) = fields.entry(name).or_default();
            *present_count += 1;
            datatypes.insert(json_datatype(&value));
        }
    }

    let fields = fields
        .into_iter()
        .map(|(name, (present_count, datatypes))| JsonlField {
            name,
            optional: present_count < row_count,
            datatype: datatypes.into_iter().collect::<Vec<_>>().join("|"),
        })
        .collect();
    let name = if path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("gz")) {
        path.file_stem().and_then(|stem| Path::new(stem).file_stem())
    } else {
        path.file_stem()
    }
    .and_then(|stem| stem.to_str())
    .unwrap_or_default()
    .to_owned();

    Ok(JsonlSchema { tables: [JsonlTable { name, fields }] })
}

fn parse_headers(path: &Path) -> Result<Vec<Column>, Box<dyn Error>> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("csv") => parse_delimited_headers(path, Some(',')),
        Some("tsv") | Some("tab") => parse_delimited_headers(path, Some('\t')),
        Some("txt") => parse_delimited_headers(path, None),
        Some("sam") => {
            sam::io::reader::Builder::default().build_from_path(path)?.read_header()?;
            Ok(fixed_columns(SAM_COLUMNS))
        }
        Some("bam") => {
            bam::io::reader::Builder.build_from_path(path)?.read_header()?;
            Ok(fixed_columns(SAM_COLUMNS))
        }
        Some("vcf") | Some("vcf.gz") => {
            let header = vcf::io::reader::Builder::default().build_from_path(path)?.read_header()?;
            Ok(vcf_columns(header.sample_names().iter()))
        }
        Some("bcf") | Some("bcf.gz") => {
            let header = bcf::io::reader::Builder::default().build_from_path(path)?.read_header()?;
            Ok(vcf_columns(header.sample_names().iter()))
        }
        Some("parquet") => {
            let mut reader = ParquetReader::new(File::open(path)?);
            let schema = reader.schema()?;
            Ok(arrow_columns(&schema))
        }
        Some("avro") => {
            let mut reader = AvroReader::new(File::open(path)?);
            Ok(polars_columns(&reader.schema()?))
        }
        Some("arrow") => {
            let mut reader = IpcReader::new(File::open(path)?);
            let schema = reader.schema()?;
            Ok(arrow_columns(&schema))
        }
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "supported extensions: csv, tsv, tab, txt, sam, bam, vcf, bcf, parquet, avro, arrow").into()),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args_os().nth(1).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "usage: parse_headers <file>"))?;
    let path = Path::new(&path);

    let jsonl = matches!(path.extension().and_then(|ext| ext.to_str()), Some("jsonl") | Some("ndjson"))
        || path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
            && matches!(path.file_stem().and_then(|stem| Path::new(stem).extension()).and_then(|ext| ext.to_str()), Some("jsonl") | Some("ndjson"));
    if jsonl {
        println!("{}", serde_json::to_string_pretty(&parse_jsonl_schema(path)?)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&ParsedSchema { columns: parse_headers(path)? })?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write, process};

    use flate2::{Compression, write::GzEncoder};
    use polars::io::{avro::AvroWriter, ipc::IpcWriter, parquet::write::ParquetWriter};

    use super::*;

    #[test]
    fn appends_vcf_sample_columns() {
        assert_eq!(vcf_columns(std::iter::empty()), fixed_columns(VCF_COLUMNS));

        let mut expected = fixed_columns(VCF_COLUMNS);
        expected.extend([Column::new("FORMAT", "TEXT"), Column::new("sample-1", "TEXT"), Column::new("sample-2", "TEXT")]);
        assert_eq!(vcf_columns([&"sample-1".into(), &"sample-2".into()].into_iter()), expected);
    }

    #[test]
    fn detects_a_likely_tabular_header() -> io::Result<()> {
        let path = env::temp_dir().join(format!("parse_headers-detection-{}.tsv", process::id()));

        fs::write(&path, "# Dataset description\nname\tage\nAlice\t42\n")?;
        assert!(likely_has_header(&path, '\t')?);

        fs::write(&path, "Alice\t42\nBob\t43\n")?;
        assert!(!likely_has_header(&path, '\t')?);

        fs::remove_file(path)
    }

    #[test]
    fn extracts_delimited_headers_or_numbers_columns() -> Result<(), Box<dyn Error>> {
        for (extension, contents, expected) in [
            (
                "csv",
                "name,age,score,active,birthday,created\n\"Doe, Alice\",42,1.5,true,2026-08-31,2026-08-31T12:30:00Z\n",
                vec![
                    Column::new("name", "TEXT"),
                    Column::new("age", "INTEGER"),
                    Column::new("score", "FLOAT"),
                    Column::new("active", "BOOLEAN"),
                    Column::new("birthday", "DATE"),
                    Column::new("created", "DATETIME"),
                ],
            ),
            ("tsv", "name\tage\nAlice\t42\n", vec![Column::new("name", "TEXT"), Column::new("age", "INTEGER")]),
            ("tab", "Alice\t42\nBob\t43\n", vec![Column::new("column_1", "TEXT"), Column::new("column_2", "INTEGER")]),
            ("txt", "name,age\nAlice,42\n", vec![Column::new("name", "TEXT"), Column::new("age", "INTEGER")]),
        ] {
            let path = env::temp_dir().join(format!("parse_headers-{}.{}", process::id(), extension));
            fs::write(&path, contents)?;
            assert_eq!(parse_headers(&path)?, expected);
            fs::remove_file(path)?;
        }

        Ok(())
    }

    #[test]
    fn extracts_headers_from_leading_comments() -> Result<(), Box<dyn Error>> {
        for (extension, contents, expected) in [
            (
                "csv",
                "# Fields:\n# ChemicalName,ChemicalID,Score\n\"A, Inc.\",C1,1.5\nB,C2,2.5\n",
                vec![Column::new("ChemicalName", "TEXT"), Column::new("ChemicalID", "TEXT"), Column::new("Score", "FLOAT")],
            ),
            ("tsv", "// Columns: name\tage\nAlice\t42\nBob\t43\n", vec![Column::new("name", "TEXT"), Column::new("age", "INTEGER")]),
            ("txt", "; Schema:\n; active,created\ntrue,2026-08-31\nfalse,2026-09-01\n", vec![Column::new("active", "BOOLEAN"), Column::new("created", "DATE")]),
        ] {
            let path = env::temp_dir().join(format!("parse_headers-comments-{}.{}", process::id(), extension));
            fs::write(&path, contents)?;
            assert_eq!(parse_headers(&path)?, expected);
            fs::remove_file(path)?;
        }

        Ok(())
    }

    #[test]
    fn extracts_parquet_headers() -> Result<(), Box<dyn Error>> {
        let path = env::temp_dir().join(format!("parse_headers-{}.parquet", process::id()));
        let mut data = df!("name" => ["Alice"], "age" => [42])?;

        ParquetWriter::new(File::create(&path)?).finish(&mut data)?;
        assert_eq!(parse_headers(&path)?, fixed_columns(&[("name", "TEXT"), ("age", "INTEGER")]));

        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn extracts_avro_headers() -> Result<(), Box<dyn Error>> {
        let path = env::temp_dir().join(format!("parse_headers-{}.avro", process::id()));
        let mut data = df!("name" => ["Alice"], "age" => [42])?;

        AvroWriter::new(File::create(&path)?).finish(&mut data)?;
        assert_eq!(parse_headers(&path)?, fixed_columns(&[("name", "TEXT"), ("age", "INTEGER")]));

        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn extracts_arrow_headers() -> Result<(), Box<dyn Error>> {
        let path = env::temp_dir().join(format!("parse_headers-{}.arrow", process::id()));
        let mut data = df!("name" => ["Alice"], "age" => [42])?;

        IpcWriter::new(File::create(&path)?).finish(&mut data)?;
        assert_eq!(parse_headers(&path)?, fixed_columns(&[("name", "TEXT"), ("age", "INTEGER")]));

        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn extracts_jsonl_schema() -> Result<(), Box<dyn Error>> {
        let path = env::temp_dir().join(format!("parse_headers-{}.jsonl", process::id()));
        fs::write(&path, "{\"id\":1,\"active\":true,\"tags\":[\"a\"],\"meta\":{\"x\":1},\"value\":null}\n\n{\"id\":2,\"active\":false,\"value\":1.5}\n")?;

        assert_eq!(
            serde_json::to_value(parse_jsonl_schema(&path)?)?,
            serde_json::json!({
                "tables": [{
                    "name": format!("parse_headers-{}", process::id()),
                    "fields": [
                        {"name": "active", "optional": false, "datatype": "boolean"},
                        {"name": "id", "optional": false, "datatype": "integer"},
                        {"name": "meta", "optional": true, "datatype": "object"},
                        {"name": "tags", "optional": true, "datatype": "array"},
                        {"name": "value", "optional": false, "datatype": "null|number"}
                    ]
                }]
            })
        );

        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn extracts_gzipped_jsonl_schema() -> Result<(), Box<dyn Error>> {
        for extension in ["jsonl.gz", "ndjson.gz"] {
            let path = env::temp_dir().join(format!("parse_headers-{}.{}", process::id(), extension));
            let mut encoder = GzEncoder::new(File::create(&path)?, Compression::default());
            encoder.write_all(b"{\"id\":1}\n{\"id\":2}\n")?;
            encoder.finish()?;

            let schema = parse_jsonl_schema(&path)?;
            assert_eq!(schema.tables[0].fields[0].datatype, "integer");
            assert_eq!(schema.tables[0].name, format!("parse_headers-{}", process::id()));
            fs::remove_file(path)?;
        }
        Ok(())
    }

    #[test]
    fn serializes_columns_as_json() -> serde_json::Result<()> {
        let schema = ParsedSchema { columns: vec![Column::new("id", "INTEGER"), Column::new("title", "TEXT")] };

        assert_eq!(
            serde_json::to_value(schema)?,
            serde_json::json!({
                "columns": [
                    {"name": "id", "datatype": "INTEGER"},
                    {"name": "title", "datatype": "TEXT"}
                ]
            })
        );
        Ok(())
    }
}
