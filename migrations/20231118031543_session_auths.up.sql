CREATE TABLE IF NOT EXISTS session_auths (
	id SERIAL PRIMARY KEY,
	sid VARCHAR(128) NOT NULL,
    user_id VARCHAR(128) NOT NULL,
    access_token VARCHAR(1024) NOT NULL,
    refresh_token VARCHAR(1024) NOT NULL,
	expiry BIGINT NOT NULL
);

CREATE INDEX idx_session_auths_sid ON session_auths(sid);
CREATE INDEX idx_session_auths_user_id ON session_auths(user_id);

