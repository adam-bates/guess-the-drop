CREATE TABLE IF NOT EXISTS csrf_tokens (
	id SERIAL PRIMARY KEY,
	sid TEXT NOT NULL,
	token TEXT NOT NULL,
	expiry INT8,
	redirect TEXT
);

CREATE INDEX IF NOT EXISTS idx_csrf_tokens_sid
ON csrf_tokens(sid);

