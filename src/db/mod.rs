use diesel::r2d2::{self, ConnectionManager};
use diesel::sqlite::SqliteConnection;

pub mod schema;
pub mod models;

pub type DbConnection = SqliteConnection;
pub type Pool = r2d2::Pool<ConnectionManager<DbConnection>>;

pub fn connect(database_url: &str) -> Pool {
    let manager = ConnectionManager::<DbConnection>::new(database_url);
    
    r2d2::Pool::builder()
        .build(manager)
        .expect("Database connection pool")
}