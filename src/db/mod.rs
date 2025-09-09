use std::sync::Arc;

use deadpool_diesel::sqlite::{Manager, Pool, Runtime};
use diesel::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use models::NoteRepository;
use tokio::sync::OnceCell;

pub mod models;
pub mod schema;

pub type DbConnection = SqliteConnection;
pub type DbPool = Pool;

// NB: Tokio's OnceCell is thread-safe
static DB_URL: OnceCell<String> = OnceCell::const_new();
static NS: OnceCell<Arc<dyn models::NoteRepository>> = OnceCell::const_new();

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub fn set_pool_url(database_url: String) -> anyhow::Result<()> {
    DB_URL.set(database_url)?;
    Ok(())
}

fn connect_pool() -> anyhow::Result<Pool> {
    let database_url = DB_URL.get().ok_or(anyhow::anyhow!("no database url provided"))?;
    let manager = Manager::new(database_url, Runtime::Tokio1);

    let pool = Pool::builder(manager)
        // .max_size(max_conn) // default is cpu * 4
        .build()?;

    Ok(pool)
}
// concrete type behind database provider which implements repository traits
pub struct DatabaseStorage {
    pool: DbPool,
}

impl DatabaseStorage {
    async fn new() -> anyhow::Result<Self> {
        let pool = connect_pool()?;
        Ok(DatabaseStorage { pool })
    }

    pub async fn note_storage() -> anyhow::Result<Arc<dyn NoteRepository>> {
        NS.get_or_try_init(async || {
            let mut db = DatabaseStorage::new().await?;
            db.run_migrations().await?;
            Ok(Arc::new(db) as Arc<dyn NoteRepository>)
        })
        .await
        .cloned()
    }

    async fn run_migrations(&mut self) -> anyhow::Result<()> {
        let _ = self
            .pool
            .get()
            .await?
            .interact(|conn| -> anyhow::Result<()> {
                let _ =
                    conn.run_pending_migrations(MIGRATIONS).map_err(anyhow::Error::from_boxed)?;

                Ok(())
            })
            .await
            .expect("Database initialization failed");

        Ok(())
    }
}
