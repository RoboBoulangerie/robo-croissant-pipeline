use std::{
    env,
    error::Error,
    fs::File,
    io::{self, BufRead, BufReader},
    path::Path,
};

use noodles::{bam, bcf, sam, vcf};
use polars::{io::avro::AvroReader, io::ipc::IpcReader, io::parquet::read::ParquetReader, prelude::*};

const SAM_HEADERS: &str = "QNAME,FLAG,RNAME,POS,MAPQ,CIGAR,RNEXT,PNEXT,TLEN,SEQ,QUAL";
const VCF_HEADERS: &str = "#CHROM,POS,ID,REF,ALT,QUAL,FILTER,INFO";

pub fn likely_has_header(path: impl AsRef<Path>, delimiter: char) -> io::Result<bool> {
    let reader = BufReader::new(File::open(path)?);
    let mut rows = Vec::with_capacity(2);

    for line in reader.lines() {
        let line = line?;

        if !line.trim().is_empty() {
            rows.push(line);
        }

        if rows.len() == 2 {
            break;
        }
    }

    if rows.len() < 2 {
        return Ok(false);
    }

    let first_row = rows[0].split(delimiter).map(str::trim).collect::<Vec<_>>();
    let second_row = rows[1].split(delimiter).map(str::trim).collect::<Vec<_>>();

    if first_row.len() != second_row.len() {
        return Ok(false);
    }

    fn looks_like_data(value: &str) -> bool {
        value.is_empty() || value.parse::<f64>().is_ok() || matches!(value.to_ascii_lowercase().as_str(), "true" | "false" | "null" | "na" | "n/a")
    }

    let first_row_values = first_row.iter().filter(|value| looks_like_data(value)).count();
    let second_row_values = second_row.iter().filter(|value| looks_like_data(value)).count();

    Ok(first_row_values < second_row_values)
}

fn parse_delimited_headers(path: &Path, delimiter: Option<char>) -> Result<String, Box<dyn Error>> {
    let first_row = BufReader::new(File::open(path)?)
        .lines()
        .find(|line| match line {
            Ok(line) => !line.trim().is_empty(),
            Err(_) => true,
        })
        .transpose()?
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "file contains no data rows"))?;
    // ponytail: text files only sniff comma vs tab; add broader detection if inputs require it.
    let delimiter = delimiter.unwrap_or_else(|| if first_row.contains('\t') { '\t' } else { ',' });
    let columns = first_row.split(delimiter).collect::<Vec<_>>();

    if likely_has_header(path, delimiter)? {
        Ok(columns.join(","))
    } else {
        Ok((1..=columns.len()).map(|index| format!("column_{index}")).collect::<Vec<_>>().join(","))
    }
}

fn parse_headers(path: &Path) -> Result<String, Box<dyn Error>> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("csv") => parse_delimited_headers(path, Some(',')),
        Some("tsv") | Some("tab") => parse_delimited_headers(path, Some('\t')),
        Some("txt") => parse_delimited_headers(path, None),
        Some("sam") => {
            sam::io::reader::Builder::default().build_from_path(path)?.read_header()?;
            Ok(SAM_HEADERS.into())
        }
        Some("bam") => {
            bam::io::reader::Builder.build_from_path(path)?.read_header()?;
            Ok(SAM_HEADERS.into())
        }
        Some("vcf") | Some("vcf.gz") => {
            let header = vcf::io::reader::Builder::default().build_from_path(path)?.read_header()?;
            Ok(with_samples(VCF_HEADERS, header.sample_names().iter()))
        }
        Some("bcf") | Some("bcf.gz") => {
            let header = bcf::io::reader::Builder::default().build_from_path(path)?.read_header()?;
            Ok(with_samples(VCF_HEADERS, header.sample_names().iter()))
        }
        Some("parquet") => {
            let mut reader = ParquetReader::new(File::open(path)?);
            Ok(reader.schema()?.iter_names().map(|name| name.as_str()).collect::<Vec<_>>().join(","))
        }
        Some("avro") => {
            let mut reader = AvroReader::new(File::open(path)?);
            Ok(reader.schema()?.iter_names().map(|name| name.as_str()).collect::<Vec<_>>().join(","))
        }
        Some("arrow") => {
            let mut reader = IpcReader::new(File::open(path)?);
            Ok(reader.schema()?.iter_names().map(|name| name.as_str()).collect::<Vec<_>>().join(","))
        }
        _ => Err(io::Error::new(io::ErrorKind::InvalidInput, "supported extensions: csv, tsv, tab, txt, sam, bam, vcf, bcf, parquet, avro, arrow").into()),
    }
}

fn with_samples<'a>(headers: &str, samples: impl ExactSizeIterator<Item = &'a String>) -> String {
    let mut headers = headers.to_owned();

    if samples.len() > 0 {
        headers.push_str(",FORMAT");
    }

    samples.fold(headers, |headers, sample| headers + "," + sample)
}

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args_os().nth(1).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "usage: parse_headers <file>"))?;

    println!("{}", parse_headers(Path::new(&path))?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{fs, process};

    use polars::io::{avro::AvroWriter, ipc::IpcWriter, parquet::write::ParquetWriter};

    use super::*;

    #[test]
    fn appends_sample_headers() {
        assert_eq!(with_samples(VCF_HEADERS, std::iter::empty()), VCF_HEADERS);
        assert_eq!(with_samples(VCF_HEADERS, [&"sample-1".into(), &"sample-2".into()].into_iter()), format!("{VCF_HEADERS},FORMAT,sample-1,sample-2"));
    }

    #[test]
    fn detects_a_likely_tabular_header() -> io::Result<()> {
        let path = env::temp_dir().join(format!("parse_headers-{}.tsv", process::id()));

        fs::write(&path, "name\tage\nAlice\t42\n")?;
        assert!(likely_has_header(&path, '\t')?);

        fs::write(&path, "Alice\t42\nBob\t43\n")?;
        assert!(!likely_has_header(&path, '\t')?);

        fs::remove_file(path)
    }

    #[test]
    fn extracts_delimited_headers_or_numbers_columns() -> Result<(), Box<dyn Error>> {
        for (extension, contents, expected) in [
            ("csv", "Alice,42\nBob,43\n", "column_1,column_2"),
            ("tsv", "name\tage\nAlice\t42\n", "name,age"),
            ("tab", "Alice\t42\nBob\t43\n", "column_1,column_2"),
            ("txt", "name,age\nAlice,42\n", "name,age"),
        ] {
            let path = env::temp_dir().join(format!("parse_headers-{}.{}", process::id(), extension));
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
        assert_eq!(parse_headers(&path)?, "name,age");

        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn extracts_avro_headers() -> Result<(), Box<dyn Error>> {
        let path = env::temp_dir().join(format!("parse_headers-{}.avro", process::id()));
        let mut data = df!("name" => ["Alice"], "age" => [42])?;

        AvroWriter::new(File::create(&path)?).finish(&mut data)?;
        assert_eq!(parse_headers(&path)?, "name,age");

        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn extracts_arrow_headers() -> Result<(), Box<dyn Error>> {
        let path = env::temp_dir().join(format!("parse_headers-{}.arrow", process::id()));
        let mut data = df!("name" => ["Alice"], "age" => [42])?;

        IpcWriter::new(File::create(&path)?).finish(&mut data)?;
        assert_eq!(parse_headers(&path)?, "name,age");

        fs::remove_file(path)?;
        Ok(())
    }
}
