use crate::blockchain::SignedBlock;
use std::{error::Error, fs::OpenOptions, io::Write};

#[cfg(test)]
pub use mockall::automock;

pub struct FileStorage {
    pub filename: String,
}

#[cfg_attr(test, automock)]
impl FileStorage {
    pub fn new(filename: &str) -> Self {
        Self {
            filename: filename.to_string(),
        }
    }
    pub fn save_block(&self, block: &SignedBlock) -> Result<(), Box<dyn Error>> {
        let json = serde_json::to_string(block)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.filename)?;
        writeln!(file, "{}", json)?;
        Ok(())
    }

    pub fn load_all_blocks(&self) -> Result<Vec<SignedBlock>, Box<dyn Error>> {
        serde_jsonlines::json_lines(&self.filename)?
            .collect::<Result<Vec<SignedBlock>, _>>()
            .map_err(|e| e.into())
    }

    pub fn load_all_blocks_if_file_exists(&self) -> Result<Vec<SignedBlock>, Box<dyn Error>> {
        if !std::path::Path::new(&self.filename).exists() {
            std::fs::File::create(&self.filename)?;
            return Ok(Vec::new());
        }

        serde_jsonlines::json_lines(&self.filename)?
            .collect::<Result<Vec<SignedBlock>, _>>()
            .map_err(|e| e.into())
    }
}
