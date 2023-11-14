CREATE TABLE IF NOT EXISTS session_auths (
	id SERIAL PRIMARY KEY,
	sid VARCHAR(128) NOT NULL,
    username VARCHAR(64) NOT NULL,
    access_token VARCHAR(1024) NOT NULL,
    refresh_token VARCHAR(1024) NOT NULL,
	expiry BIGINT NOT NULL
);

CREATE INDEX idx_session_auths_sid ON session_auths(sid);

