use std::{
    env,
    path::{Path, PathBuf},
    process,
};

use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

mod db;
mod service;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    db_path: PathBuf,
    bind_addr: String,
    secret_key: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        let db_path = default_db_path().expect("Could not determine config directory");
        Self {
            db_path,
            bind_addr: "0.0.0.0:3030".to_string(),
            secret_key: None,
        }
    }
}

fn default_db_path() -> Option<PathBuf> {
    let data_dir = dirs::data_dir()
        .map(|dir| dir.join("domotux"))?;
    Some(data_dir.join("domotux.db"))
}

#[derive(Parser)]
struct Cli {
    #[clap(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Initialize {
        db_path: Option<PathBuf>,
    },
    CreateUser,
    GenSecret {
        #[clap(short, long)]
        write_to_config: bool,
        #[clap(short, long)]
        clipboard: bool,
    },
}

#[tokio::main]
async fn main() -> process::ExitCode {
    env_logger::init();

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
    let mut config: Config = base::cfg::load_config("domotux", None).await?;

    match &cli.command {
        Some(Command::Initialize { db_path }) => {
            initialize(&mut config, db_path.as_deref()).await?;
        }
        Some(Command::CreateUser) => {
            create_user(&config.db_path).await?;
        }
        Some(Command::GenSecret {
            write_to_config,
            clipboard,
        }) => {
            generate_secret_key(&mut config, *write_to_config, *clipboard).await?;
        }
        None => {
            start_service(config).await?;
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

    if config.secret_key.is_none() && env::var_os("DOMOTUX_SECRET_KEY").is_none() {
        eprintln!("No secret key configured. To start the service you must set a secret key.");
        eprintln!(
            "This can be done by setting the DOMOTUX_SECRET_KEY environment variable or by adding a 'secret_key' field to the config file."
        );
        eprintln!("The command 'gen-secret' can be used to generate a random secret key.");
    }

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

async fn generate_secret_key(
    config: &mut Config,
    write_to_config: bool,
    clipboard: bool,
) -> anyhow::Result<()> {
    use base64::prelude::*;
    use rand::RngExt;

    let mut rng = rand::rng();
    let key: [u8; 32] = rng.random();
    let secret_key = BASE64_STANDARD.encode(key);

    let mut done = false;

    if write_to_config {
        println!("Updating config file with new secret key.");
        config.secret_key = Some(secret_key.clone());
        base::cfg::save_config("domotux", &config, None).await?;
        done = true;
    }

    if clipboard {
        if let Err(e) = arboard::Clipboard::new()
            .and_then(|mut clipboard| clipboard.set_text(secret_key.clone()))
        {
            log::error!("Failed to copy secret key to clipboard: {}", e);
        } else {
            log::info!("Secret key copied to clipboard.");
        }
        done = true;
    }

    if !done {
        println!("{}", secret_key);
    }

    Ok(())
}

async fn start_service(config: Config) -> anyhow::Result<()> {
    log::info!("Starting domotux service");

    let secret_key = config
        .secret_key
        .clone()
        .or_else(|| env::var("DOMOTUX_SECRET_KEY").ok())
        .ok_or_else(|| anyhow::anyhow!(
            "No secret key configured. Set the DOMOTUX_SECRET_KEY environment variable or add a 'secret_key' field to the config file."))?;

    if secret_key.len() < 12 {
        anyhow::bail!("Secret key must be at least 12 characters long. Use the 'gen-secret' command to generate a valid secret key.");
    }

    let config = Config { secret_key: Some(secret_key), ..config };

    service::start(&config).await?;
    Ok(())
}
