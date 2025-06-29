use diesel::{prelude::*, r2d2::{ConnectionManager, PooledConnection}};
use notes::Note;
use super::{schema, DbConnection};

pub mod notes;

pub struct NoteStorage {
    conn: PooledConnection<ConnectionManager<DbConnection>>,
}

impl NoteStorage {
    pub fn new(conn: PooledConnection<ConnectionManager<DbConnection>>) -> Self {
        NoteStorage { 
            conn,
        }
    }
}

// TODO: should be generic over storage 
// TODO: get rid of &mut use async connection pooler (no good for sqlite because of blocking api)
pub trait Storable {
    fn add_note(&mut self, note: Note) -> QueryResult<usize>;
    fn get_notes(&mut self) -> QueryResult<Vec<Note>>;
    fn get_note_by_id(&mut self, note_id: &str) -> QueryResult<Option<Note>>;
    fn delete_note_by_id(&mut self, note_id: &str) -> QueryResult<usize>;
}

impl Storable for NoteStorage {
    fn add_note(&mut self, note: Note) -> QueryResult<usize> {
        diesel::insert_into(schema::notes::table)
            .values(&note)
            .execute(&mut self.conn)
    }

    fn get_notes(&mut self) -> QueryResult<Vec<Note>> {
        schema::notes::table.load::<Note>(&mut self.conn)
    }

    fn get_note_by_id(&mut self, note_id: &str) -> QueryResult<Option<Note>> {
        schema::notes::table
            .filter(schema::notes::note_id.eq(note_id))
            .first::<Note>(&mut self.conn)
            .optional()
    }

    fn delete_note_by_id(&mut self, note_id: &str) -> QueryResult<usize> {
        diesel::delete(schema::notes::table.filter(schema::notes::note_id.eq(note_id)))
            .execute(&mut self.conn)
    }
}