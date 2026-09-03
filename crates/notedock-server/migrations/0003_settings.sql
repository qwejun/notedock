-- Server settings that must survive container upgrades, including the
-- first-run password hash.
CREATE TABLE app_settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
