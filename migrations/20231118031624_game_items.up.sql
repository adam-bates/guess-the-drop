CREATE TABLE IF NOT EXISTS game_items (
	game_item_id SERIAL PRIMARY KEY,
    game_code VARCHAR(128) NOT NULL,

    name VARCHAR(1024) NOT NULL,
    image VARCHAR(1024),

	enabled BOOLEAN NOT NULL
);

CREATE INDEX idx_game_items_game_code ON game_items(game_code);

