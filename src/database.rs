use crate::models::Submission;
use anyhow::Result;
use rusqlite::{params, Connection};

pub struct Database {
    path: String,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        Ok(Self {
            path: path.to_string(),
        })
    }

    fn get_connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.path)?)
    }

    pub fn init(&self) -> Result<()> {
        let conn = self.get_connection()?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS submissions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER,
                user_email TEXT,
                user_full_name TEXT,
                submission_name TEXT,
                timestamp TEXT,
                file_checksum TEXT,
                file_path TEXT,
                expected_gain REAL,
                actual_gain REAL,
                tp INTEGER,
                tn INTEGER,
                fp INTEGER,
                fn INTEGER,
                positives_predicted INTEGER,
                threshold_category TEXT,
                after_deadline INTEGER DEFAULT 0
            )",
            [],
        )?;

        Ok(())
    }

    pub fn save_submission(&self, submission: &Submission) -> Result<i64> {
        let conn = self.get_connection()?;

        conn.execute(
            "INSERT INTO submissions (
                user_id, user_email, user_full_name, submission_name,
                timestamp, file_checksum, file_path, expected_gain, actual_gain,
                tp, tn, fp, fn, positives_predicted, threshold_category, after_deadline
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                submission.user_id,
                submission.user_email,
                submission.user_full_name,
                submission.submission_name,
                submission.timestamp,
                submission.file_checksum,
                submission.file_path,
                submission.expected_gain,
                submission.actual_gain,
                submission.tp,
                submission.tn,
                submission.fp,
                submission.fn_,
                submission.positives_predicted,
                submission.threshold_category,
                submission.after_deadline as i32,
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    pub fn get_user_submissions(&self, user_id: i64) -> Result<Vec<Submission>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, user_id, user_email, user_full_name, submission_name,
                    timestamp, file_checksum, file_path, expected_gain, actual_gain,
                    tp, tn, fp, fn, positives_predicted, threshold_category, after_deadline
             FROM submissions
             WHERE user_id = ?1
             ORDER BY timestamp DESC",
        )?;

        let submissions = stmt
            .query_map([user_id], |row| {
                Ok(Submission {
                    id: Some(row.get(0)?),
                    user_id: row.get(1)?,
                    user_email: row.get(2)?,
                    user_full_name: row.get(3)?,
                    submission_name: row.get(4)?,
                    timestamp: row.get(5)?,
                    file_checksum: row.get(6)?,
                    file_path: row.get(7)?,
                    expected_gain: row.get(8)?,
                    actual_gain: row.get(9)?,
                    tp: row.get(10)?,
                    tn: row.get(11)?,
                    fp: row.get(12)?,
                    fn_: row.get(13)?,
                    positives_predicted: row.get(14)?,
                    threshold_category: row.get(15)?,
                    after_deadline: row.get::<_, i32>(16)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(submissions)
    }

    pub fn get_duplicates(&self) -> Result<Vec<(String, i32, String, String)>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT file_checksum, COUNT(*),
                    GROUP_CONCAT(DISTINCT user_full_name) as users,
                    GROUP_CONCAT(submission_name) as names
             FROM submissions
             GROUP BY file_checksum
             HAVING COUNT(DISTINCT user_id) > 1",
        )?;

        let duplicates = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(duplicates)
    }

    pub fn get_leaderboard(&self) -> Result<Vec<(String, String, String, f64, i32, Option<f64>)>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "WITH last_valid_submission AS (
                SELECT 
                    user_id,
                    user_full_name,
                    user_email,
                    actual_gain,
                    timestamp,
                    ROW_NUMBER() OVER (
                        PARTITION BY user_id 
                        ORDER BY timestamp DESC
                    ) as rn
                FROM submissions
                WHERE after_deadline = 0
            ),
            user_stats AS (
                SELECT
                    s.user_id,
                    s.user_full_name,
                    s.user_email,
                    s.timestamp,
                    COUNT(*) as total_submissions,
                    lvs.actual_gain as final_gain,
                    MAX(CASE WHEN s.after_deadline = 0 THEN s.actual_gain END) as max_gain
                FROM submissions s
                LEFT JOIN last_valid_submission lvs 
                    ON s.user_id = lvs.user_id AND lvs.rn = 1
                GROUP BY s.user_id, s.user_full_name, s.user_email, lvs.actual_gain
            )
            SELECT
                user_full_name,
                user_email,
                timestamp,
                final_gain,
                total_submissions,
                max_gain
            FROM user_stats
            WHERE final_gain IS NOT NULL
            ORDER BY final_gain DESC",
        )?;

        let results = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, f64>(3)?,
                    row.get::<_, i32>(4)?,
                    row.get::<_, Option<f64>>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(results)
    }

    pub fn get_user_submissions_by_identifier(&self, identifier: &str) -> Result<Vec<Submission>> {
        let conn = self.get_connection()?;
        let pattern = format!("%{}%", identifier);
        let mut stmt = conn.prepare(
            "SELECT id, user_id, user_email, user_full_name, submission_name,
                    timestamp, file_checksum, file_path, expected_gain, actual_gain,
                    tp, tn, fp, fn, positives_predicted, threshold_category, after_deadline
             FROM submissions
             WHERE user_email LIKE ?1 OR user_full_name LIKE ?1
             ORDER BY timestamp DESC",
        )?;

        let submissions = stmt
            .query_map([&pattern], |row| {
                Ok(Submission {
                    id: Some(row.get(0)?),
                    user_id: row.get(1)?,
                    user_email: row.get(2)?,
                    user_full_name: row.get(3)?,
                    submission_name: row.get(4)?,
                    timestamp: row.get(5)?,
                    file_checksum: row.get(6)?,
                    file_path: row.get(7)?,
                    expected_gain: row.get(8)?,
                    actual_gain: row.get(9)?,
                    tp: row.get(10)?,
                    tn: row.get(11)?,
                    fp: row.get(12)?,
                    fn_: row.get(13)?,
                    positives_predicted: row.get(14)?,
                    threshold_category: row.get(15)?,
                    after_deadline: row.get::<_, i32>(16)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(submissions)
    }

    pub fn get_all_submissions(&self) -> Result<Vec<Submission>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(
            "SELECT id, user_id, user_email, user_full_name, submission_name,
                    timestamp, file_checksum, file_path, expected_gain, actual_gain,
                    tp, tn, fp, fn, positives_predicted, threshold_category, after_deadline
             FROM submissions
             ORDER BY timestamp DESC",
        )?;

        let submissions = stmt
            .query_map([], |row| {
                Ok(Submission {
                    id: Some(row.get(0)?),
                    user_id: row.get(1)?,
                    user_email: row.get(2)?,
                    user_full_name: row.get(3)?,
                    submission_name: row.get(4)?,
                    timestamp: row.get(5)?,
                    file_checksum: row.get(6)?,
                    file_path: row.get(7)?,
                    expected_gain: row.get(8)?,
                    actual_gain: row.get(9)?,
                    tp: row.get(10)?,
                    tn: row.get(11)?,
                    fp: row.get(12)?,
                    fn_: row.get(13)?,
                    positives_predicted: row.get(14)?,
                    threshold_category: row.get(15)?,
                    after_deadline: row.get::<_, i32>(16)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(submissions)
    }
}
