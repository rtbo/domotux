use std::path::{Path, PathBuf};
use std::process;

use clap::{Parser, Subcommand};
use mqtt::BrokerAddress;
use serde::{Deserialize, Serialize};

mod db;
mod service;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    broker: BrokerAddress,
    db_path: PathBuf,
    bind_addr: String,
}

impl Default for Config {
    fn default() -> Self {
        let db_path = default_db_path().expect("Could not determine config directory");
        Self {
            broker: BrokerAddress::default(),
            db_path,
            bind_addr: "0.0.0.0:3030".to_string(),
        }
    }
}

fn default_db_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("domotux").join("domotux.db"))
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
    Initialize { db_path: Option<PathBuf> },
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
        Some(Command::Initialize { db_path }) => {
            initialize(&mut config, db_path.as_deref()).await?;
        }
        Some(Command::CreateUser) => {
            create_user(&config.db_path).await?;
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

async fn initialize(config: &mut Config, db_path_cli: Option<&Path>) -> anyhow::Result<()> {
    let update_config_path = db_path_cli.is_some();
    let db_path = db_path_cli
        .as_deref()
        .unwrap_or(&config.db_path)
        .to_path_buf();
    let parent = db_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid database path: {}", db_path.display()))?;
    tokio::fs::create_dir_all(parent).await?;
    let db = db::Db::open(db_path.clone()).await?;

    db.initialize().await?;

    if update_config_path {
        println!(
            "Updating config file with new database path: {}",
            db_path.display()
        );
        config.db_path = db_path.clone();
        base::cfg::save_config("domotux", &config, None).await?;
    }
    println!("Database initialized successfully at {}", db_path.display());
    println!("You can now create a user with the 'create-user' command.");

    Ok(())
}

async fn create_user(db_path: &Path) -> anyhow::Result<()> {
    let username = inquire::Text::new("Username:").prompt()?;
    validate_username(&username)?;

    let password = inquire::Password::new("Password:").prompt()?;
    validate_password(&password)?;

    let db = db::Db::open(db_path.to_path_buf()).await?;
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
