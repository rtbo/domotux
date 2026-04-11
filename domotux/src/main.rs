use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

mod db;
mod service;

#[derive(Debug, Clone, Serialize, Deserialize)]
enum WeekStart {
    Monday,
    Sunday,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    broker: mqtt::BrokerAddress,
    bind_addr: String,
    tls: Option<TlsConfig>,
    day_start: Option<base::DayTime>,
    week_start: WeekStart,
    influx: influx::Config,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TlsConfig {
    cert_path: String,
    key_path: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker: Default::default(),
            bind_addr: "0.0.0.0:3030".to_string(),
            tls: None,
            day_start: Some("06:00".parse().unwrap()),
            week_start: WeekStart::Monday,
            influx: influx::Config::default(),
        }
    }
}

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Option<Command>,

    #[clap(long)]
    default_config: bool,

    #[clap(short, long)]
    broker: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    LocateDb,
    Initialize,
    CreateUser,
}

#[tokio::main]
async fn main() -> process::ExitCode {
    let cli = Cli::parse();

    if let Err(e) = run(cli).await {
        eprintln!("Error: {}", e);
        if std::env::var_os("RUST_BACKTRACE").is_some() {
            eprintln!("{}", e.backtrace());
        }
        process::ExitCode::FAILURE
    } else {
        process::ExitCode::SUCCESS
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    if cli.default_config {
        base::cfg::print_default_config::<Config>()?;
        return Ok(());
    }

    let mut config: Config = base::cfg::load_config("domotux", None).await?;

    match &cli.command {
        Some(Command::Initialize) => {
            initialize().await?;
        }
        Some(Command::CreateUser) => {
            create_user().await?;
        }
        Some(Command::LocateDb) => {
            for db_path in possible_db_paths() {
                if db_path.exists() {
                    println!("Database found at {}", db_path.display());
                    break;
                } else {
                    println!("No database found at {}", db_path.display());
                }
            }
        }
        None => {
            if let Some(broker) = cli.broker {
                config.broker = broker.parse()?;
            }
            service::start(&config).await?;
        }
    }
    Ok(())
}

fn possible_db_paths() -> Vec<PathBuf> {
    let mut paths = vec![PathBuf::from("/var/lib/domotux/domotux.db")];
    if let Some(home_db_path) = dirs::data_dir().map(|d| d.join("domotux/domotux.db")) {
        paths.push(home_db_path);
    }
    paths
}

async fn initialize() -> anyhow::Result<()> {
    for db_path in possible_db_paths() {
        if let Err(e) = try_initialize(&db_path).await {
            eprintln!("Failed to initialize database at {}: {}", db_path.display(), e);
        } else {
            return Ok(());
        }
    }

    anyhow::bail!("Failed to initialize database at all attempted paths.");
}

async fn try_initialize(db_path: &Path) -> anyhow::Result<()> {
    let parent = db_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid database path: {}", db_path.display()))?;
    tokio::fs::create_dir_all(parent).await?;

    let db = db::Db::open(&db_path).await?;

    db.initialize().await?;

    println!("Database initialized successfully at {}", db_path.display());
    println!("You can now create a user with the 'create-user' command.");

    Ok(())
}

async fn find_and_open_db() -> anyhow::Result<db::Db> {
    for db_path in possible_db_paths() {
        match db::Db::open(&db_path).await {
            Ok(db) => return Ok(db),
            Err(e) => eprintln!("Failed to open database at {}: {}", db_path.display(), e),
        }
    }

    anyhow::bail!("Failed to open database at all attempted paths.");
}

async fn create_user() -> anyhow::Result<()> {
    let db = find_and_open_db().await?;

    let username = inquire::Text::new("Username:").prompt()?;
    validate_username(&username)?;

    let password = inquire::Password::new("Password:").prompt()?;
    validate_password(&password)?;

    db.create_user(&username, &password).await?;
    println!("User '{}' created successfully.", username);
    Ok(())
}

fn validate_username(username: &str) -> anyhow::Result<()> {
    if username.len() < 3 {
        anyhow::bail!("Username must be at least 3 characters long");
    }
    // User name should only contain [a-zA-Z0-9_-] characters
    if !username
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        anyhow::bail!("Username can only contain letters, numbers, underscores, and hyphens");
    }
    // User name should not start with a number
    if username.chars().next().unwrap().is_numeric() {
        anyhow::bail!("Username cannot start with a number");
    }
    Ok(())
}

fn validate_password(password: &str) -> anyhow::Result<()> {
    if password.len() < 3 {
        anyhow::bail!("Password must be at least 3 characters long");
    }
    Ok(())
}
