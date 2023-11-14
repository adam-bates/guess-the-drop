CREATE TABLE IF NOT EXISTS csrf_tokens (
	id SERIAL PRIMARY KEY,
	sid VARCHAR(128) NOT NULL,
	token VARCHAR(1024) NOT NULL,
	expiry BIGINT,
	redirect VARCHAR(1024)
);

CREATE INDEX idx_csrf_tokens_sid ON csrf_tokens(sid);

