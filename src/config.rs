use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    pub zulip: ZulipConfig,
    pub database: DatabaseConfig,
    pub logs: LogsConfig,
    pub teachers: Vec<String>,
    pub master_data: MasterDataConfig,
    pub submissions: SubmissionsConfig,
    pub gain_matrix: GainMatrix,
    pub gain_thresholds: Vec<GainThreshold>,
    pub competition: CompetitionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZulipConfig {
    pub email: String,
    pub api_key: String,
    pub site: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterDataConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionsConfig {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainMatrix {
    pub tp: f64,
    pub tn: f64,
    pub fp: f64,
    pub fn_: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GainThreshold {
    pub min_gain: f64,
    pub category: String,
    pub message: String,
    #[serde(default)]
    pub gifs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompetitionConfig {
    pub name: String,
    pub description: String,
    pub deadline: String,
    pub results_reveal_date: String,
}

impl BotConfig {
    pub fn load(path: &str) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;

        let config: BotConfig =
            serde_json::from_str(&content).with_context(|| "Failed to parse config file")?;

        Ok(config)
    }
}

pub fn create_config_template() -> Result<()> {
    let config = BotConfig {
        zulip: ZulipConfig {
            email: "dosesfinges@example.com".to_string(),
            api_key: "your-api-key-here".to_string(),
            site: "https://your-org.zulipchat.com".to_string(),
        },
        database: DatabaseConfig {
            path: "dos_esfinges.db".to_string(),
        },
        logs: LogsConfig {
            path: "logs".to_string(),
        },
        teachers: vec![
            "teacher1@example.com".to_string(),
            "teacher2@example.com".to_string(),
        ],
        master_data: MasterDataConfig {
            path: "master_data.csv".to_string(),
        },
        submissions: SubmissionsConfig {
            path: "./submissions".to_string(),
        },
        gain_matrix: GainMatrix {
            tp: 1.0,
            tn: 0.5,
            fp: -0.1,
            fn_: -0.5,
        },
        gain_thresholds: vec![
            GainThreshold {
                min_gain: 100.0,
                category: "excellent".to_string(),
                message: "¡Modelo excepcional!".to_string(),
                gifs: vec![
                    "https://media.giphy.com/media/v1.Y2lkPTc5MGI3NjExYWJj/giphy.gif".to_string(),
                    "https://media.giphy.com/media/v1.Y2lkPTc5MGI3NjExZGVm/giphy.gif".to_string(),
                ],
            },
            GainThreshold {
                min_gain: 50.0,
                category: "good".to_string(),
                message: "Buen trabajo".to_string(),
                gifs: vec![
                    "https://media.giphy.com/media/v1.Y2lkPTc5MGI3NjExZ2hp/giphy.gif".to_string(),
                ],
            },
            GainThreshold {
                min_gain: 0.0,
                category: "basic".to_string(),
                message: "Sigue intentando".to_string(),
                gifs: vec![
                    "https://media.giphy.com/media/v1.Y2lkPTc5MGI3NjExamp/giphy.gif".to_string(),
                ],
            },
        ],
        competition: CompetitionConfig {
            name: "Competencia ML - Dos Esfinges".to_string(),
            description: "Competencia de machine learning usando DosEsfingesBot".to_string(),
            deadline: "2025-12-31T23:59:59".to_string(),
            results_reveal_date: "2026-01-01T23:59:59".to_string(),
        },
    };

    let json = serde_json::to_string_pretty(&config)?;
    fs::write("config.json", json)?;

    Ok(())
}
