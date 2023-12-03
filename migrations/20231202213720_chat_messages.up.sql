CREATE TABLE IF NOT EXISTS chat_messages (
    id SERIAL PRIMARY KEY,
    game_code VARCHAR(128) NOT NULL,
	message VARCHAR(1024) NOT NULL,
    lock_id VARCHAR(128),
    sent BOOLEAN NOT NULL
);

CREATE INDEX idx_chat_messages_game_code ON chat_messages(game_code);

