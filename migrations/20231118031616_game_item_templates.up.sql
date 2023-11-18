CREATE TABLE IF NOT EXISTS game_item_templates (
	game_item_template_id SERIAL PRIMARY KEY,
    game_template_id BIGINT UNSIGNED NOT NULL,

    name VARCHAR(1024) NOT NULL,
    image VARCHAR(1024),

	start_enabled BOOLEAN NOT NULL
);

CREATE INDEX idx_game_item_templates_game_template_id ON game_item_templates(game_template_id);

