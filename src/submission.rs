use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use csv::ReaderBuilder;
use rand::seq::SliceRandom;
use regex::Regex;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tracing::{info, warn};

use crate::config::BotConfig;
use crate::database::Database;
use crate::master_data::MasterData;
use crate::models::{GainResult, Message, Submission};

pub async fn process_submit(
    message: &Message,
    config: &BotConfig,
    db: &Database,
    master_data: &MasterData,
    is_teacher: bool,
) -> String {
    let user_email = &message.sender_email;

    info!(
        "Processing submit from {} (teacher: {})",
        user_email, is_teacher
    );

    // Check deadline - be more flexible with date parsing
    let deadline = match DateTime::parse_from_rfc3339(&config.competition.deadline) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => {
            // Try parsing without timezone
            match chrono::NaiveDateTime::parse_from_str(
                &config.competition.deadline,
                "%Y-%m-%dT%H:%M:%S",
            ) {
                Ok(naive_dt) => chrono::DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc),
                Err(e) => {
                    warn!(
                        "Invalid deadline format '{}': {}",
                        config.competition.deadline, e
                    );
                    // Default to far future if parse fails
                    chrono::DateTime::<Utc>::from_naive_utc_and_offset(
                        chrono::NaiveDateTime::new(
                            chrono::NaiveDate::from_ymd_opt(2099, 12, 31).unwrap(),
                            chrono::NaiveTime::from_hms_opt(23, 59, 59).unwrap(),
                        ),
                        Utc,
                    )
                }
            }
        }
    };
    let after_deadline = Utc::now() > deadline;

    // Parse command
    let parts: Vec<&str> = message.content.trim().split_whitespace().collect();
    if parts.len() < 3 {
        return "❌ Formato incorrecto. Uso: `submit <nombre_envio> <ganancia_esperada>` y adjunta el archivo CSV".to_string();
    }

    let submission_name = parts[1].to_string();
    let expected_gain: f64 = match parts[2].parse() {
        Ok(g) => g,
        Err(_) => return "❌ La ganancia esperada debe ser un número".to_string(),
    };

    info!(
        "Submission name: {}, Expected gain: {}",
        submission_name, expected_gain
    );

    // Extract file from message
    let (filename, file_content) = match extract_file_from_message(&message.content, config).await {
        Ok(Some((f, c))) => (f, c),
        Ok(None) => {
            return "❌ Debes adjuntar un archivo CSV. Usa el formato: `submit <nombre> <ganancia_esperada>` y adjunta el archivo CSV.".to_string();
        }
        Err(e) => {
            return format!("❌ Error descargando archivo: {}", e);
        }
    };

    if !filename.to_lowercase().ends_with(".csv") {
        return "❌ El archivo debe ser un CSV".to_string();
    }

    // Save file
    let file_path = match save_submission_file(
        &message.sender_full_name,
        &submission_name,
        &filename,
        &file_content,
        is_teacher,
        config,
    ) {
        Ok(p) => p,
        Err(e) => return format!("❌ Error guardando archivo: {}", e),
    };

    // Calculate checksum
    let checksum = calculate_checksum(&file_content);
    info!(
        "File saved: {}, checksum: {}...",
        file_path,
        &checksum[..16]
    );

    // Read and validate CSV
    let predicted_ids = match read_csv_ids(&file_content) {
        Ok(ids) => ids,
        Err(e) => return format!("❌ Error leyendo CSV: {}", e),
    };

    // Validate IDs
    let invalid_ids = master_data.validate_ids(&predicted_ids);
    if !invalid_ids.is_empty() {
        warn!("Invalid IDs in submission from {}", user_email);
        return format!(
            "❌ IDs inválidos encontrados: {} IDs no existen en el dataset",
            invalid_ids.len()
        );
    }

    // Calculate gain
    info!("Calculating gain for {}", submission_name);
    let gain_result = calculate_gain(&predicted_ids, master_data, &config.gain_matrix);
    let threshold_category = get_threshold_category(gain_result.gain, config);
    let positives_predicted = predicted_ids.len() as i32;

    info!(
        "Gain calculated - Expected: {:.4}, Actual: {:.4}",
        expected_gain, gain_result.gain
    );

    // Create submission record
    let submission = Submission {
        id: None,
        user_id: message.sender_id,
        user_email: user_email.clone(),
        user_full_name: message.sender_full_name.clone(),
        submission_name: submission_name.clone(),
        timestamp: Utc::now().to_rfc3339(),
        file_checksum: checksum,
        file_path,
        expected_gain,
        actual_gain: gain_result.gain,
        tp: gain_result.tp,
        tn: gain_result.tn,
        fp: gain_result.fp,
        fn_: gain_result.fn_,
        positives_predicted,
        threshold_category: threshold_category.clone(),
        after_deadline,
    };

    // Save to database
    let submission_id = match db.save_submission(&submission) {
        Ok(id) => id,
        Err(e) => return format!("❌ Error guardando envío: {}", e),
    };

    info!("Submission saved with ID: {}", submission_id);

    // Build response
    let threshold_config = config
        .gain_thresholds
        .iter()
        .find(|t| t.category == threshold_category)
        .unwrap();

    let mut response = format!("🎯 **{}**\n\n", threshold_config.message);
    response.push_str(&format!("🆔 **ID Envío:** {}\n", submission_id));
    response.push_str(&format!("📊 **Ganancia esperada:** {:.4}\n", expected_gain));

    // Teachers see actual gain
    if is_teacher {
        response.push_str(&format!("✨ **Ganancia real:** {:.4}\n", gain_result.gain));
        response.push_str(&format!(
            "📈 **Positivos predichos:** {}\n",
            positives_predicted
        ));
        response.push_str(&format!(
            "🔢 **Matriz confusión:** TP={}, TN={}, FP={}, FN={}\n",
            gain_result.tp, gain_result.tn, gain_result.fp, gain_result.fn_
        ));
    }

    // After deadline notification
    if after_deadline {
        response.push_str("\n⚠️ **ENVÍO FUERA DE PLAZO** - Registrado pero no compite\n");
    }

    // Add random GIF
    if !threshold_config.gifs.is_empty() {
        let mut rng = rand::thread_rng();
        if let Some(gif) = threshold_config.gifs.choose(&mut rng) {
            response.push_str(&format!("\n{}", gif));
        }
    }

    response
}

pub fn process_list_submits(user_id: i64, db: &Database) -> String {
    let submissions = match db.get_user_submissions(user_id) {
        Ok(s) => s,
        Err(e) => return format!("❌ Error obteniendo envíos: {}", e),
    };

    if submissions.is_empty() {
        return "📋 No tienes envíos registrados".to_string();
    }

    let mut response = "📋 **Tus Envíos:**\n\n".to_string();
    response.push_str("| ID | Nombre | 📅 Fecha | 💰 Esperada | 🎯 Categoría | ⏰ |\n");
    response.push_str("|---|---|---|---|---|---|\n");

    for sub in submissions {
        let deadline_mark = if sub.after_deadline { "⚠️" } else { "✅" };
        let ts_str: String = sub.timestamp.chars().take(16).collect();
        response.push_str(&format!(
            "|{}|{}|{}|{:.2}|{}|{}|\n",
            sub.id.unwrap_or(0),
            sub.submission_name,
            ts_str,
            sub.expected_gain,
            sub.threshold_category,
            deadline_mark
        ));
    }

    response
}

pub fn process_duplicates(db: &Database) -> String {
    let duplicates = match db.get_duplicates() {
        Ok(d) => d,
        Err(e) => return format!("❌ Error obteniendo duplicados: {}", e),
    };

    if duplicates.is_empty() {
        return "✅ No se encontraron envíos duplicados".to_string();
    }

    let mut response = "🔍 **Envíos Duplicados:**\n\n".to_string();
    for (checksum, _count, users, names) in duplicates {
        response.push_str(&format!("**Checksum:** `{}...`\n", &checksum[..16]));
        response.push_str(&format!("**Usuarios:** {}\n", users));
        response.push_str(&format!("**Envíos:** {}\n\n", names));
    }

    response
}

pub fn process_leaderboard_full(db: &Database, config: &BotConfig, order_by: &str) -> String {
    let results = match db.get_leaderboard(order_by) {
        Ok(r) => r,
        Err(e) => return format!("❌ Error obteniendo leaderboard: {}", e),
    };

    if results.is_empty() {
        return "📊 No hay submissions en el leaderboard".to_string();
    }

    let order_label = match order_by {
        "datetime" => "Ordenado por Fecha",
        _ => "Ordenado por Ganancia",
    };

    let mut response = format!(
        "🏆 **Leaderboard Completo - {} ({})** \n\n",
        config.competition.name,
        order_label
    );
    response.push_str("| Pos | Nombre | TS | 💰 Elegido | 💰 Esperada | 📊 Envíos | 📈 Máximo |\n");
    response.push_str("|---|---|---|---|---|---|---|\n");

    for (i, (name, email, ts, best_gain, expected_gain, total, max_gain)) in results.iter().enumerate() {
        if !config.teachers.contains(email) {
            let max_str = max_gain
                .map(|a| format!("{:.2}", a))
                .unwrap_or_else(|| "N/A".to_string());
            let ts_str: String = ts.chars().take(16).collect();
            response.push_str(&format!(
                "| {} | {} | {} | {:.2} | {:.2} | {} | {} |\n",
                i + 1,
                name,
                ts_str,
                best_gain,
                expected_gain,
                total,
                max_str
            ));
        }
    }

    response
}

pub fn process_user_submits(user_identifier: &str, db: &Database) -> String {
    let submissions = match db.get_user_submissions_by_identifier(user_identifier) {
        Ok(s) => s,
        Err(e) => return format!("❌ Error obteniendo envíos: {}", e),
    };

    if submissions.is_empty() {
        return format!("📋 No se encontraron envíos para '{}'", user_identifier);
    }

    let mut response = format!("📋 **Envíos de '{}':**\n\n", user_identifier);
    response.push_str("| ID | Nombre | 📅 Fecha | 💰 Esperada | ✨ Real | 🎯 | ⏰ |\n");
    response.push_str("|---|---|---|---|---|---|---|\n");

    for sub in submissions {
        let deadline_mark = if sub.after_deadline { "⚠️" } else { "✅" };
        let ts_str: String = sub.timestamp.chars().take(16).collect();
        response.push_str(&format!(
            "|{}|{}|{}|{:.2}|{:.2}|{}|{}|\n",
            sub.id.unwrap_or(0),
            sub.submission_name,
            ts_str,
            sub.expected_gain,
            sub.actual_gain,
            sub.threshold_category,
            deadline_mark
        ));
    }

    response
}

pub fn process_user_submits_by_id(user_id: i64, db: &Database) -> String {
    let submissions = match db.get_user_submissions(user_id) {
        Ok(s) => s,
        Err(e) => return format!("❌ Error obteniendo envíos: {}", e),
    };

    if submissions.is_empty() {
        return "📋 No se encontraron envíos para el usuario mencionado".to_string();
    }

    // Get user info from first submission
    let user_name = if !submissions[0].user_full_name.is_empty() {
        &submissions[0].user_full_name
    } else {
        &submissions[0].user_email
    };

    let mut response = format!("📋 **Envíos de {}:**\n\n", user_name);
    response.push_str("| ID | Nombre | 📅 Fecha | 💰 Esperada | ✨ Real | 🎯 | ⏰ |\n");
    response.push_str("|---|---|---|---|---|---|---|\n");

    for sub in submissions {
        let deadline_mark = if sub.after_deadline { "⚠️" } else { "✅" };
        let ts_str: String = sub.timestamp.chars().take(16).collect();
        response.push_str(&format!(
            "|{}|{}|{}|{:.2}|{:.2}|{}|{}|\n",
            sub.id.unwrap_or(0),
            sub.submission_name,
            ts_str,
            sub.expected_gain,
            sub.actual_gain,
            sub.threshold_category,
            deadline_mark
        ));
    }

    response
}

pub fn process_all_submits(db: &Database) -> String {
    let submissions = match db.get_all_submissions() {
        Ok(s) => s,
        Err(e) => return format!("❌ Error obteniendo envíos: {}", e),
    };

    if submissions.is_empty() {
        return "📋 No hay envíos registrados en el sistema".to_string();
    }

    let mut response = "📋 **Todos los Envíos del Sistema:**\n\n".to_string();
    response.push_str("| ID | Usuario | Nombre | 📅 Fecha | 💰 Esperada | ✨ Real | 🎯 | ⏰ |\n");
    response.push_str("|---|---|---|---|---|---|---|---|\n");

    for sub in submissions {
        let deadline_mark = if sub.after_deadline { "⚠️" } else { "✅" };
        let ts_str: String = sub.timestamp.chars().take(16).collect();
        response.push_str(&format!(
            "|{}|{}|{}|{}|{:.2}|{:.2}|{}|{}|\n",
            sub.id.unwrap_or(0),
            if sub.user_full_name.is_empty() {
                &sub.user_email
            } else {
                &sub.user_full_name
            },
            sub.submission_name,
            ts_str,
            sub.expected_gain,
            sub.actual_gain,
            sub.threshold_category,
            deadline_mark
        ));
    }

    response
}

// Helper functions

async fn extract_file_from_message(
    content: &str,
    config: &BotConfig,
) -> Result<Option<(String, Vec<u8>)>> {
    let re = Regex::new(r"\[([^\]]+\.csv)\]\(([^)]+)\)")?;

    if let Some(caps) = re.captures(content) {
        let filename = caps.get(1).unwrap().as_str().to_string();
        let url = caps.get(2).unwrap().as_str();

        let full_url = if url.starts_with("http") {
            url.to_string()
        } else {
            format!("{}{}", config.zulip.site, url)
        };

        let client = reqwest::Client::new();
        let response = client
            .get(&full_url)
            .basic_auth(&config.zulip.email, Some(&config.zulip.api_key))
            .send()
            .await?;

        let content = response.bytes().await?.to_vec();
        Ok(Some((filename, content)))
    } else {
        Ok(None)
    }
}

fn save_submission_file(
    user_name: &str,
    submission_name: &str,
    filename: &str,
    content: &[u8],
    is_teacher: bool,
    config: &BotConfig,
) -> Result<String> {
    let base_path = PathBuf::from(&config.submissions.path);

    let safe_user_name: String = user_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect();

    let user_dir = if is_teacher {
        base_path.join("teachers").join(safe_user_name)
    } else {
        base_path.join("students").join(safe_user_name)
    };

    fs::create_dir_all(&user_dir)?;

    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let safe_name: String = submission_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect();

    let file_path = user_dir.join(format!("{}_{}_{}", timestamp, safe_name.trim(), filename));

    fs::write(&file_path, content)?;

    Ok(file_path.to_string_lossy().to_string())
}

fn calculate_checksum(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    hex::encode(hasher.finalize())
}

fn read_csv_ids(content: &[u8]) -> Result<HashSet<i32>> {
    let mut reader = ReaderBuilder::new().has_headers(false).from_reader(content);

    let mut ids = HashSet::new();

    for result in reader.records() {
        let record = result?;
        if record.len() != 1 {
            anyhow::bail!("CSV must have exactly 1 column");
        }

        let id: i32 = record[0]
            .parse()
            .with_context(|| format!("Invalid ID: {}", &record[0]))?;
        ids.insert(id);
    }

    Ok(ids)
}

fn calculate_gain(
    predicted_ids: &HashSet<i32>,
    master_data: &MasterData,
    gain_matrix: &crate::config::GainMatrix,
) -> GainResult {
    let mut tp = 0;
    let mut tn = 0;
    let mut fp = 0;
    let mut fn_ = 0;

    for id in master_data.all_ids() {
        let is_positive = master_data.positive_ids().contains(id);
        let predicted_positive = predicted_ids.contains(id);

        match (is_positive, predicted_positive) {
            (true, true) => tp += 1,
            (true, false) => fn_ += 1,
            (false, true) => fp += 1,
            (false, false) => tn += 1,
        }
    }

    let gain = (tp as f64) * gain_matrix.tp
        + (tn as f64) * gain_matrix.tn
        + (fp as f64) * gain_matrix.fp
        + (fn_ as f64) * gain_matrix.fn_;

    GainResult {
        gain,
        tp,
        tn,
        fp,
        fn_,
    }
}

fn get_threshold_category(gain: f64, config: &BotConfig) -> String {
    let mut thresholds = config.gain_thresholds.clone();
    thresholds.sort_by(|a, b| b.min_gain.partial_cmp(&a.min_gain).unwrap());

    for threshold in thresholds.iter() {
        if gain >= threshold.min_gain {
            return threshold.category.clone();
        }
    }

    thresholds.last().unwrap().category.clone()
}
