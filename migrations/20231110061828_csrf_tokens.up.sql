CREATE TABLE IF NOT EXISTS csrf_tokens (
	id SERIAL PRIMARY KEY,
	sid TEXT,
	token TEXT not null,
	ttl INT
);

CREATE INDEX IF NOT EXISTS idx_csrf_tokens_sid
ON csrf_tokens(sid);

