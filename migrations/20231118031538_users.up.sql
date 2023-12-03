CREATE TABLE IF NOT EXISTS users (
	user_id VARCHAR(128) NOT NULL PRIMARY KEY,
	username VARCHAR(128) NOT NULL,
	twitch_login VARCHAR(128) NOT NULL
);

CREATE INDEX idx_users_username ON users(username);

