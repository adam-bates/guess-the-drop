CREATE TABLE IF NOT EXISTS game_templates (
	game_template_id SERIAL PRIMARY KEY,
    user_id VARCHAR(128) NOT NULL,

    name VARCHAR(1024) NOT NULL,
    reward_message VARCHAR(1024)
);

CREATE INDEX idx_game_templates_user_id ON game_templates(user_id);

