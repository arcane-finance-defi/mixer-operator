use crate::db::schema;
use chrono::NaiveDateTime;
use diesel::{
    AsExpression, FromSqlRow, backend::Backend, deserialize, prelude::*, serialize,
    sql_types::Integer,
};

#[derive(Queryable, Insertable, AsChangeset, QueryableByName, Selectable)]
#[diesel(table_name = schema::notes)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct FullNote {
    pub note_id: String, // TODO: this should be indexable to use with indexing or even miden_objects type directly
    pub note: String,
    pub account_id: String,
    pub scheduled_datetime: Option<NaiveDateTime>,
    pub status: NoteStatus,
}

#[derive(Insertable)]
#[diesel(table_name = schema::notes)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
pub struct NewNote<'a> {
    pub note_id: &'a str,
    pub note: &'a str,
    pub account_id: &'a str,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, AsExpression, FromSqlRow)]
    #[diesel(sql_type = Integer)]
    pub struct NoteStatus: u8 {
        const ACCEPTED = 0x01;
        const RECONSTRUCTED = 0x02;
        const ONCHAIN = 0x04;
        const TXED = 0x08;
        const CONSUMED = 0x30;
    }
}

impl<DB> deserialize::FromSql<Integer, DB> for NoteStatus
where
    DB: Backend,
    i32: deserialize::FromSql<Integer, DB>,
{
    fn from_sql(bytes: DB::RawValue<'_>) -> deserialize::Result<Self> {
        let bits = i32::from_sql(bytes)?;
        Ok(NoteStatus::from_bits_retain(bits as u8))
    }
}

// SQLite specific implementation due to 'b lifetime specific with temporary values
// Refer to ToSql docs
impl serialize::ToSql<Integer, diesel::sqlite::Sqlite> for NoteStatus
where
    i32: serialize::ToSql<Integer, diesel::sqlite::Sqlite>,
{
    fn to_sql<'b>(
        &'b self,
        out: &mut serialize::Output<'b, '_, diesel::sqlite::Sqlite>,
    ) -> serialize::Result {
        out.set_value(self.bits() as i32);
        Ok(serialize::IsNull::No)
    }
}
