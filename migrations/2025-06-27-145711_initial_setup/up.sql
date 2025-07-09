/* Create table for notes' "drafts" */
/* Use TEXT type because storing serialized representation */
CREATE TABLE notes (
    note_id TEXT PRIMARY KEY UNIQUE NOT NULL,
    note TEXT NOT NULL,
    account_id TEXT NOT NULL
);
