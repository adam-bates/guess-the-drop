CREATE TABLE IF NOT EXISTS game_items (
	game_item_id SERIAL PRIMARY KEY,
    game_id BIGINT UNSIGNED NOT NULL,

    name VARCHAR(1024) NOT NULL,
    image VARCHAR(1024),

	enabled BOOLEAN NOT NULL
);

CREATE INDEX idx_game_items_game_id ON game_items(game_id);

