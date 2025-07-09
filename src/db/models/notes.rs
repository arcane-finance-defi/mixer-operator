use crate::db::schema;
use chrono::NaiveDateTime;
use diesel::prelude::*;

#[derive(Queryable, Insertable, AsChangeset, QueryableByName, Selectable)]
#[diesel(table_name = schema::notes)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct Note {
    pub note_id: String, // TODO: this should be indexable to use with indexing or even miden_objects type directly
    pub note: String,
    pub account_id: String,
    pub scheduled_datetime: Option<NaiveDateTime>,
    pub status: i32,
}
