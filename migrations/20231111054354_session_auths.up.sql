CREATE TABLE IF NOT EXISTS session_auths (
	id SERIAL PRIMARY KEY,
	sid TEXT NOT NULL,
    username TEXT NOT NULL,
    access_token TEXT NOT NULL,
    refresh_token TEXT NOT NULL,
	expiry INT8 NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_session_auths_sid
ON session_auths(sid);

