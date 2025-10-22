use dos_esfinges_bot::config::BotConfig;
use reqwest::Client;
use serde_json::json;
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 DosEsfingesBot - Diagnostics Tool");
    println!("====================================");
    println!();

    // Load config
    let config = BotConfig::load("config.json")?;

    println!("📧 Bot Email: {}", config.zulip.email);
    println!("🌐 Site: {}", config.zulip.site);
    println!(
        "🔑 API Key: {}...",
        &config.zulip.api_key[..10.min(config.zulip.api_key.len())]
    );
    println!();

    let client = Client::new();

    // Test 1: Check credentials
    println!("Test 1: Checking credentials...");
    let url = format!("{}/api/v1/users/me", config.zulip.site);

    let response = client
        .get(&url)
        .basic_auth(&config.zulip.email, Some(&config.zulip.api_key))
        .send()
        .await?;

    let status = response.status();
    let body: serde_json::Value = response.json().await?;

    if status.is_success() && body["result"] == "success" {
        println!("✅ Credentials are valid");
        println!("   Bot ID: {}", body["user_id"]);
        println!("   Bot Name: {}", body["full_name"]);
        println!("   Bot Email: {}", body["email"]);
    } else {
        println!("❌ Invalid credentials");
        println!("   Status: {}", status);
        println!("   Response: {}", body);
        return Ok(());
    }
    println!();

    // Test 2: Send test message
    println!("Test 2: Testing message sending...");
    print!("Enter recipient email (or press Enter to skip): ");
    io::stdout().flush()?;

    let mut recipient = String::new();
    io::stdin().read_line(&mut recipient)?;
    let recipient = recipient.trim();

    if !recipient.is_empty() {
        let url = format!("{}/api/v1/messages", config.zulip.site);

        // Zulip API requires form data, not JSON
        let params = [
            ("type", "private"),
            ("to", recipient),
            (
                "content",
                "🧪 Test message from DosEsfingesBot diagnostics - Connection OK!",
            ),
        ];

        let response = client
            .post(&url)
            .basic_auth(&config.zulip.email, Some(&config.zulip.api_key))
            .form(&params)
            .send()
            .await?;

        let status = response.status();
        let body: serde_json::Value = response.json().await?;

        if status.is_success() && body["result"] == "success" {
            println!("✅ Test message sent successfully");
            println!("   Message ID: {}", body["id"]);
        } else {
            println!("❌ Failed to send message");
            println!("   Status: {}", status);
            println!("   Response: {}", body);
            return Ok(());
        }
    } else {
        println!("⚠️  Skipping message test");
    }
    println!();

    // Test 3: Event registration
    println!("Test 3: Testing event registration...");
    let url = format!("{}/api/v1/register", config.zulip.site);

    let payload = json!({
        "event_types": ["message"]
    });

    let response = client
        .post(&url)
        .basic_auth(&config.zulip.email, Some(&config.zulip.api_key))
        .json(&payload)
        .send()
        .await?;

    let status = response.status();
    let body: serde_json::Value = response.json().await?;

    if status.is_success() && body["result"] == "success" {
        println!("✅ Event registration successful");
        println!("   Queue ID: {}", body["queue_id"]);
    } else {
        println!("❌ Event registration failed");
        println!("   Status: {}", status);
        println!("   Response: {}", body);
        return Ok(());
    }
    println!();

    // Test 4: Fetch events
    println!("Test 4: Testing event fetching...");
    let queue_id = body["queue_id"].as_str().unwrap();
    let url = format!("{}/api/v1/events", config.zulip.site);

    let response = client
        .get(&url)
        .basic_auth(&config.zulip.email, Some(&config.zulip.api_key))
        .query(&[("queue_id", queue_id), ("last_event_id", "-1")])
        .send()
        .await?;

    let status = response.status();
    let body: serde_json::Value = response.json().await?;

    if status.is_success() && body["result"] == "success" {
        println!("✅ Event fetching successful");
        let events = body["events"].as_array().unwrap();
        println!("   Events received: {}", events.len());
    } else {
        println!("❌ Event fetching failed");
        println!("   Status: {}", status);
        println!("   Response: {}", body);
        return Ok(());
    }
    println!();

    println!("====================================");
    println!("✅ All tests passed!");
    println!("====================================");
    println!();
    println!("The bot should be able to:");
    println!("  ✓ Authenticate with Zulip");
    println!("  ✓ Send private messages");
    println!("  ✓ Register for events");
    println!("  ✓ Receive events");
    println!();
    println!("If the bot still doesn't respond, check:");
    println!("  1. The bot is running: ps aux | grep dos_esfinges_bot");
    println!("  2. You're sending PRIVATE messages (not @mentions in streams)");
    println!("  3. Check logs: tail -f logs/dos_esfinges_bot_*.log");
    println!("  4. Run with debug: RUST_LOG=debug ./target/release/dos_esfinges_bot run --config config.json");
    println!();

    Ok(())
}
