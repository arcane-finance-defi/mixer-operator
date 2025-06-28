use diesel::prelude::*;
use crate::db::schema;

#[derive(Queryable, Insertable, AsChangeset, QueryableByName)]
#[diesel(table_name = schema::notes)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Note {
    pub note_id: String, // TODO: this should be indexable to use with indexing
    pub note: String,
    pub account_id: String,
}