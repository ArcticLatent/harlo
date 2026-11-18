-- Users who sign in.
CREATE TABLE IF NOT EXISTS users (
    id serial primary key, -- the id
    name text
);

CREATE OR REPLACE FUNCTION total(a int) RETURNS int AS $$
BEGIN
    RETURN a; -- done
END;
$$ LANGUAGE plpgsql;

CREATE VIEW active AS SELECT * FROM users;
CREATE INDEX users_name ON users (name);
