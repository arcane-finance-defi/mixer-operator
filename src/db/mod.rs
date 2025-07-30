use deadpool_diesel::sqlite::{Runtime, Manager, Pool};
use diesel::SqliteConnection;

pub mod models;
pub mod schema;

pub type DbConnection = SqliteConnection;
pub type DbPool = Pool;

pub fn connect_pool(database_url: &str) -> anyhow::Result<Pool> {
    let manager = Manager::new(database_url, Runtime::Tokio1);

    let pool = Pool::builder(manager)
        // .max_size(max_conn) // default is cpu * 4
        .build()?;

    Ok(pool)
}

