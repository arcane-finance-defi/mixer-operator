// @generated automatically by Diesel CLI.

diesel::table! {
    notes (note_id) {
        note_id -> Text,
        note -> Text,
        account_id -> Text,
    }
}
