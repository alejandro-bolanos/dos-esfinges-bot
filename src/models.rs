use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub sender_email: String,
    pub sender_id: i64,
    pub sender_full_name: String,
    pub content: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: i64,
    #[serde(rename = "type")]
    pub event_type: String,
    pub message: Option<Message>,
}

#[derive(Debug, Clone)]
pub struct Submission {
    pub id: Option<i64>,
    pub user_id: i64,
    pub user_email: String,
    pub user_full_name: String,
    pub submission_name: String,
    pub timestamp: String,
    pub file_checksum: String,
    pub file_path: String,
    pub expected_gain: f64,
    pub actual_gain: f64,
    pub tp: i32,
    pub tn: i32,
    pub fp: i32,
    pub fn_: i32,
    pub positives_predicted: i32,
    pub threshold_category: String,
    pub after_deadline: bool,
}

#[derive(Debug, Clone)]
pub struct GainResult {
    pub gain: f64,
    pub tp: i32,
    pub tn: i32,
    pub fp: i32,
    pub fn_: i32,
}

#[derive(Debug, Deserialize)]
pub struct ZulipEventsResponse {
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ZulipUser {
    pub user_id: i64,
    pub full_name: String,
    pub email: String,
    #[serde(default)]
    pub is_bot: bool,
    #[serde(default)]
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct ZulipUsersResponse {
    pub members: Vec<ZulipUser>,
}

#[derive(Debug, Deserialize)]
pub struct ZulipUserPresence {
    #[serde(default)]
    pub timestamp: i64,
}
