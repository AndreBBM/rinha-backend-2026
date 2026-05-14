use flate2::read::GzDecoder;
use serde::Deserialize;
use std::fs::{File};
use std::io::{BufWriter, BufReader, Write};

#[derive(Deserialize, Debug)]
struct MyData {
    // Define fields matching your JSON structure
    vector: [f32; 14],
    label: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Open the file
    let file = File::open("references.json.gz")?;
    
    // 2. Wrap in a Gzip decoder
    let decoder = GzDecoder::new(file);
    
    // 3. Wrap in a BufReader for efficient reading
    let reader = BufReader::new(decoder);

    // 4. Parse the JSON
    // Deserialize as an array of records
    //let stream = serde_json::Deserializer::from_reader(reader).into_iter::<MyData>();
    let data: Vec<MyData> = serde_json::from_reader(reader)?;
    
    // Cria ou abre o arquivo para escrita
    let file = File::create("test.bin")?;

    let mut writer = BufWriter::new(file);

    for record in data {
        // 1. Escrever os 14 floats como bytes (Little Endian)
        for &val in &record.vector {
            writer.write_all(&val.to_le_bytes())?;
        }

        // 2. Escrever o label como um único byte (1 = fraude, 0 = legítimo)
        if record.label == "fraud" {
            writer.write_all(&[1u8])?;
        } else {
            writer.write_all(&[0u8])?;
        }
    }
    writer.flush()?;
    
    Ok(())
    
}