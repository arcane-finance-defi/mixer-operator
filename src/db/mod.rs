use deadpool_diesel::sqlite::{Manager, Pool, Runtime};
use diesel::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

pub mod models;
pub mod schema;

pub type DbConnection = SqliteConnection;
pub type DbPool = Pool;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub fn connect_pool(database_url: &str) -> anyhow::Result<Pool> {
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
    pub fn new(pool: DbPool) -> Self {
        DatabaseStorage { pool }
    }

    pub async fn initialize(&mut self) -> anyhow::Result<()> {
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
