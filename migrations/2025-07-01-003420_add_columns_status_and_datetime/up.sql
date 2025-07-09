ALTER TABLE notes ADD COLUMN scheduled_datetime DATETIME;
ALTER TABLE notes ADD COLUMN status INTEGER CHECK (status >= 0 AND status <= 255) NOT NULL;