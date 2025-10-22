use crate::models::{Event, ZulipEventsResponse};
use anyhow::Result;
use reqwest::Client;

pub struct ZulipClient {
    email: String,
    api_key: String,
    site: String,
    client: Client,
    queue_id: Option<String>,
}

impl ZulipClient {
    pub fn new(email: String, api_key: String, site: String) -> Self {
        Self {
            email,
            api_key,
            site,
            client: Client::new(),
            queue_id: None,
        }
    }

    async fn register_queue(&mut self) -> Result<String> {
        let url = format!("{}/api/v1/register", self.site);

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.email, Some(&self.api_key))
            .form(&[("event_types", r#"["message"]"#)])
            .send()
            .await?;

        let data: serde_json::Value = response.json().await?;

        Ok(data["queue_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("No queue_id in response"))?
            .to_string())
    }

    pub async fn get_events(&mut self, last_event_id: i64) -> Result<Vec<Event>> {
        if self.queue_id.is_none() {
            let queue_id = self.register_queue().await?;
            self.queue_id = Some(queue_id);
        }

        let queue_id = self.queue_id.as_ref().unwrap();
        let url = format!("{}/api/v1/events", self.site);

        let response = self
            .client
            .get(&url)
            .basic_auth(&self.email, Some(&self.api_key))
            .query(&[
                ("queue_id", queue_id.as_str()),
                ("last_event_id", &last_event_id.to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            // Queue might have expired, re-register
            self.queue_id = None;
            return Ok(vec![]);
        }

        let data: ZulipEventsResponse = response.json().await?;
        Ok(data.events)
    }

    pub async fn send_message(&self, to: &str, content: &str) -> Result<()> {
        use tracing::{error, info};

        let url = format!("{}/api/v1/messages", self.site);

        info!("Sending message to: {}", to);
        info!("Message length: {} chars", content.len());

        // Zulip expects form data, not JSON
        let params = [("type", "private"), ("to", to), ("content", content)];

        let response = self
            .client
            .post(&url)
            .basic_auth(&self.email, Some(&self.api_key))
            .form(&params)
            .send()
            .await?;

        let status = response.status();
        info!("Response status: {}", status);

        if !status.is_success() {
            let error_body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            error!(
                "Failed to send message. Status: {}, Body: {}",
                status, error_body
            );
            anyhow::bail!("Failed to send message: {} - {}", status, error_body);
        }

        info!("Message sent successfully to {}", to);
        Ok(())
    }

    pub async fn download_file(&self, url: &str) -> Result<Vec<u8>> {
        let full_url = if url.starts_with("http") {
            url.to_string()
        } else {
            format!("{}{}", self.site, url)
        };

        let response = self
            .client
            .get(&full_url)
            .basic_auth(&self.email, Some(&self.api_key))
            .send()
            .await?;

        Ok(response.bytes().await?.to_vec())
    }
}
