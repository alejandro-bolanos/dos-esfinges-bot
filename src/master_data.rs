use anyhow::{Context, Result};
use csv::ReaderBuilder;
use std::collections::HashSet;
use std::fs::File;

pub struct MasterData {
    all_ids: HashSet<i32>,
    positive_ids: HashSet<i32>,
}

impl MasterData {
    pub fn load(path: &str) -> Result<Self> {
        let file = File::open(path)
            .with_context(|| format!("Failed to open master data file: {}", path))?;

        let mut reader = ReaderBuilder::new().has_headers(true).from_reader(file);

        let mut all_ids = HashSet::new();
        let mut positive_ids = HashSet::new();

        for result in reader.records() {
            let record = result?;

            if record.len() < 2 {
                anyhow::bail!(
                    "Invalid CSV format: expected at least 2 columns (id, clase_binaria)"
                );
            }

            let id: i32 = record[0]
                .parse()
                .with_context(|| format!("Invalid ID: {}", &record[0]))?;

            let clase: i32 = record[1]
                .parse()
                .with_context(|| format!("Invalid clase_binaria: {}", &record[1]))?;

            all_ids.insert(id);

            if clase == 1 {
                positive_ids.insert(id);
            }
        }

        Ok(Self {
            all_ids,
            positive_ids,
        })
    }

    pub fn validate_ids(&self, predicted_ids: &HashSet<i32>) -> Vec<i32> {
        predicted_ids
            .iter()
            .filter(|id| !self.all_ids.contains(id))
            .copied()
            .collect()
    }

    pub fn all_ids(&self) -> &HashSet<i32> {
        &self.all_ids
    }

    pub fn positive_ids(&self) -> &HashSet<i32> {
        &self.positive_ids
    }

    pub fn total_count(&self) -> usize {
        self.all_ids.len()
    }

    pub fn positive_count(&self) -> usize {
        self.positive_ids.len()
    }
}
