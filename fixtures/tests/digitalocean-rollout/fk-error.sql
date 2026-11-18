CREATE TABLE parent(value TEXT);
CREATE TABLE child(value TEXT REFERENCES parent(value));
