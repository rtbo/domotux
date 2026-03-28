use std::path::PathBuf;

use rand::RngExt;
use sha2::Digest;

#[derive(Debug)]
pub struct Db {
    path: PathBuf,
    db: turso::Database,
}

impl Db {
    pub async fn open(path: PathBuf) -> anyhow::Result<Self> {
        let path_str = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid path: {}", path.display()))?;
        let db = turso::Builder::new_local(path_str).build().await?;
        Ok(Self { path, db })
    }

    pub async fn initialize(&self) -> anyhow::Result<()> {
        let conn = self.db.connect()?;

        // Initialize the database schema here

        // verify that the database does not already contain a schema
        let mut tables = conn
            .query("SELECT name FROM sqlite_master WHERE type='table'", ())
            .await?;
        if tables.next().await?.is_some() {
            anyhow::bail!(
                "Database already contains a schema. Delete {} before reinitializing.",
                self.path.display()
            );
        }

        conn.execute(
            "CREATE TABLE users (
                name TEXT PRIMARY KEY,
                pwd BLOB NOT NULL,
                salt BLOB NOT NULL
            )",
            (),
        )
        .await?;

        // verify that the database has been initialized correctly
        let mut tables = conn
            .query("SELECT name FROM sqlite_master WHERE type='table'", ())
            .await?;
        let mut has_users_table = false;
        while let Some(row) = tables.next().await? {
            let table_name = row.get_value(0)?.as_text().map(|s| s.to_string());
            println!("Found table: {:?}", table_name);
            if table_name.as_deref() == Some("users") {
                has_users_table = true;
                break;
            }
        }
        if !has_users_table {
            anyhow::bail!("Failed to initialize database schema. 'users' table not found.");
        }

        Ok(())
    }

    pub async fn create_user(&self, username: &str, password: &str) -> anyhow::Result<()> {
        let conn = self.db.connect()?;

        // Check if the user already exists
        let mut stmt = conn
            .prepare("SELECT 1 FROM users WHERE username = ?")
            .await?;
        let mut rows = stmt.query((username,)).await?;
        if rows.next().await?.is_some() {
            anyhow::bail!("User {} already exists", username);
        }

        // Create the user
        let salt = generate_salt();
        let pwd_hash = hash_password(password, &salt);
        conn.query(
            "INSERT INTO users (name, pwd, salt) VALUES (?, ?, ?)",
            (username, pwd_hash.as_slice(), salt.as_slice()),
        )
        .await?;

        Ok(())
    }

    pub async fn auth_user(&self, username: &str, password: &str) -> anyhow::Result<bool> {
        let conn = self.db.connect()?;

        let mut stmt = conn
            .prepare("SELECT pwd, salt FROM users WHERE name = ?")
            .await?;
        let mut rows = stmt.query((username,)).await?;

        if let Some(row) = rows.next().await? {
            let stored_pwd = row.get_value(0)?;
            let salt = row.get_value(1)?;

            let stored_pwd = stored_pwd.as_blob().ok_or_else(|| {
                anyhow::anyhow!("Invalid password hash for user {}", username)
            })?;
            let salt = salt.as_blob().ok_or_else(|| {
                anyhow::anyhow!("Invalid salt for user {}", username)
            })?;

            let input_pwd_hash = hash_password(password, salt);
            Ok(stored_pwd == input_pwd_hash.as_slice())
        } else {
            Ok(false)
        }
    }
}

fn generate_salt() -> [u8; 16] {
    let mut rng = rand::rng();
    rng.random()
}

fn hash_password(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut hasher = sha2::Sha256::new();
    hasher.update(password.as_bytes());
    hasher.update(salt);

    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    hash
}
