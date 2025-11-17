use dos_esfinges_bot::{config, database, master_data, models, submission, zulip};

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::{error, info};

use chrono::Local;
use std::fs;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use config::BotConfig;
use database::Database;
use master_data::MasterData;
use zulip::ZulipClient;

#[derive(Parser)]
#[command(name = "dos_esfinges_bot")]
#[command(about = "Bot de Zulip para competencias tipo Kaggle", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a config template
    CreateConfig,
    /// Run the bot
    Run {
        /// Config file path
        #[arg(short, long)]
        config: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup tracing with both stdout and file output
    let log_dir = std::path::PathBuf::from("logs");
    fs::create_dir_all(&log_dir)?;

    let log_file = log_dir.join(format!(
        "dos_esfinges_bot_{}.log",
        Local::now().format("%Y%m%d")
    ));

    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)?;

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    // Console layer (stdout) - colorful and concise
    let console_layer = fmt::layer()
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .compact();

    // File layer - detailed with timestamps
    let file_layer = fmt::layer()
        .with_writer(std::sync::Arc::new(file))
        .with_target(true)
        .with_ansi(false)
        .with_line_number(true)
        .with_thread_ids(true);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::CreateConfig) => {
            config::create_config_template()?;
            info!("Config template created successfully at config.json");
            Ok(())
        }
        Some(Commands::Run { config }) => run_bot(&config).await,
        None => {
            if let Some(config_path) = cli.config {
                run_bot(&config_path).await
            } else {
                eprintln!("Please specify a config file or use --help");
                Ok(())
            }
        }
    }
}

async fn run_bot(config_path: &str) -> Result<()> {
    info!("Starting DosEsfingesBot with config: {}", config_path);

    // Load configuration
    let config = BotConfig::load(config_path)?;
    info!("Configuration loaded successfully");

    // Initialize database
    let db = Database::new(&config.database.path)?;
    db.init()?;
    info!("Database initialized at: {}", config.database.path);

    // Load master data
    let master_data = MasterData::load(&config.master_data.path)?;
    info!(
        "Master data loaded: {} records, {} positives",
        master_data.total_count(),
        master_data.positive_count()
    );

    // Create Zulip client
    let client = ZulipClient::new(
        config.zulip.email.clone(),
        config.zulip.api_key.clone(),
        config.zulip.site.clone(),
    );

    info!("Competition: {}", config.competition.name);
    info!("Deadline: {}", config.competition.deadline);
    info!("Teachers: {}", config.teachers.len());
    info!("Bot ready! Listening for private messages...");

    // Start message loop
    let bot = Bot {
        config,
        client,
        db,
        master_data,
    };

    let mut bot = bot;
    bot.run().await?;

    Ok(())
}

struct Bot {
    config: BotConfig,
    client: ZulipClient,
    db: Database,
    master_data: MasterData,
}

impl Bot {
    async fn run(&mut self) -> Result<()> {
        let mut last_event_id = -1;

        loop {
            match self.client.get_events(last_event_id).await {
                Ok(events) => {
                    for event in events {
                        if event.event_type == "message" {
                            if let Some(message) = event.message {
                                if message.msg_type == "private"
                                    && message.sender_email != self.config.zulip.email
                                {
                                    self.handle_message(message).await;
                                }
                            }
                        }
                        last_event_id = event.id;
                    }
                }
                Err(e) => {
                    error!("Error fetching events: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
    }

    async fn handle_message(&self, message: models::Message) {
        let sender_email = message.sender_email.clone();
        let content = message.content.trim().to_lowercase();

        info!(
            "Message from {}: {}",
            sender_email,
            if content.len() > 50 {
                format!("{}...", &content[..50])
            } else {
                content.clone()
            }
        );

        let is_teacher = self.config.teachers.contains(&sender_email);
        info!("User is teacher: {}", is_teacher);

        let response = if content.starts_with("submit ") && !is_teacher {
            info!("Processing submit command (student)");
            submission::process_submit(
                &message,
                &self.config,
                &self.db,
                &self.master_data,
                is_teacher,
            )
            .await
        } else if content.starts_with("submit ") && is_teacher {
            info!("Submit command blocked for teacher");
            "⚠️ Los profesores no pueden enviar submissions. Usa los comandos de administración."
                .to_string()
        } else if content == "list submits" && !is_teacher {
            info!("Processing list submits command");
            submission::process_list_submits(message.sender_id, &self.db)
        } else if content == "duplicates" && is_teacher {
            info!("Processing duplicates command (teacher)");
            submission::process_duplicates(&self.db)
        } else if content.starts_with("leaderboard") && is_teacher {
            info!("Processing leaderboard command (teacher)");
            let parts: Vec<&str> = message.content.trim().split_whitespace().collect();
            let order_by = if parts.len() >= 2 {
                match parts[1].to_lowercase().as_str() {
                    "datetime" => "datetime",
                    "gain" => "gain",
                    _ => "gain", // default to gain for invalid options
                }
            } else {
                "gain" // default to gain
            };
            submission::process_leaderboard_full(&self.db, &self.config, order_by)
        } else if content == "all submits" && is_teacher {
            info!("Processing all submits command (teacher)");
            submission::process_all_submits(&self.db)
        } else if content == "no submits" && is_teacher {
            info!("Processing no submits command (teacher)");
            submission::process_no_submits(&self.db, &self.client, &self.config).await
        } else if content.starts_with("user submits") && is_teacher {
            info!("Processing user submits command (teacher)");
            // Extract user_id from Zulip mention format: @**Name|user_id**
            if let Some(user_id) = self.extract_mentioned_user_id(&message.content) {
                submission::process_user_submits_by_id(user_id, &self.db)
            } else {
                "❌ Uso: user submits @usuario (usa la mención de Zulip)".to_string()
            }
        } else if content == "help" {
            info!("Processing help command");
            self.get_help_message(is_teacher)
        } else {
            info!("Unknown command, showing help");
            self.get_help_message(is_teacher)
        };

        info!("Response generated, length: {} chars", response.len());
        info!("Attempting to send message to: {}", sender_email);

        match self.client.send_message(&sender_email, &response).await {
            Ok(_) => {
                info!("✅ Response sent successfully to {}", sender_email);
            }
            Err(e) => {
                error!("❌ Error sending message to {}: {}", sender_email, e);
            }
        }
    }

    fn extract_mentioned_user_id(&self, content: &str) -> Option<i64> {
        // Zulip mentions come in format: @**Name|user_id** or @**Name**
        // We need to extract the user_id from the pipe format
        use regex::Regex;
        let re = Regex::new(r"@\*\*[^|]+\|(\d+)\*\*").unwrap();
        
        if let Some(captures) = re.captures(content) {
            if let Some(user_id_str) = captures.get(1) {
                return user_id_str.as_str().parse::<i64>().ok();
            }
        }
        None
    }

    fn get_help_message(&self, is_teacher: bool) -> String {
        let comp = &self.config.competition;

        if is_teacher {
            format!(
                "🤖 **DosEsfingesBot - Ayuda para Profesores**\n\n\
                **Competencia:** {}\n\
                **Descripción:** {}\n\
                **Fecha límite:** {}\n\n\
                **Comandos disponibles:**\n\
                • `duplicates` - Listar envíos duplicados\n\
                • `leaderboard [gain|datetime]` - Leaderboard completo con estadísticas (ordenado por ganancia o fecha)\n\
                • `all submits` - Ver todos los envíos del sistema\n\
                • `no submits` - Ver usuarios sin envíos ordenados por última conexión\n\
                • `user submits @usuario` - Ver envíos de un usuario (usa mención @)\n\
                • `help` - Mostrar esta ayuda\n\n\
                **Nota:** Los profesores no pueden enviar submissions.",
                comp.name, comp.description, comp.deadline
            )
        } else {
            format!(
                "🤖 **DosEsfingesBot - Ayuda para Estudiantes**\n\n\
                **Competencia:** {}\n\
                **Descripción:** {}\n\
                **Fecha límite:** {}\n\n\
                **Comandos disponibles:**\n\
                • `submit <nombre> <ganancia_esperada>` - Enviar modelo (adjuntar CSV)\n\
                • `list submits` - Listar tus envíos\n\
                • `help` - Mostrar esta ayuda\n\n\
                **Formato CSV:** 1 columna con los IDs que predices como positivos (sin encabezado)",
                comp.name, comp.description, comp.deadline
            )
        }
    }
}
