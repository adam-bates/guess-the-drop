use std::time::SystemTime;

use super::*;

use crate::{
    models::{
        Game, GameItem, GameItemOutcome, GameItemTemplate, GamePlayer, GameTemplate,
        GameWithHostedSummary, GameWithJoinedSummary, User, GAME_STATUS_ACTIVE,
        GAME_STATUS_FINISHED,
    },
    prelude::*,
};

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post, put},
    Form, Router,
};
use serde::Deserialize;
use tower_sessions::Session;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    return router
        .route("/join", get(join))
        .route("/games", get(games).post(post_game))
        .route("/games/:game_code", get(game))
        .route("/games/:game_code/finish", post(finish_game))
        .route("/games/:game_code/x/lock", put(game_x_lock))
        .route("/games/:game_code/x/unlock", put(game_x_unlock))
        .route(
            "/games/:game_code/items/:game_item_id/x/enable",
            put(game_x_enable_item),
        )
        .route(
            "/games/:game_code/items/:game_item_id/x/disable",
            put(game_x_disable_item),
        )
        .route(
            "/games/:game_code/items/:game_item_id/x/choose",
            put(game_x_choose_item),
        );
}

#[derive(Debug, Deserialize)]
struct JoinParams {
    code: Option<String>,
}

#[derive(Template)]
#[template(path = "join.html")]
struct JoinTemplate {
    game_code: String,
}

async fn join(
    Query(params): Query<JoinParams>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let Some(game_code) = params.code else {
        return Ok(Redirect::to("/").into_response());
    };

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }

    let game_code = game_code.to_lowercase();

    let game = sqlx::query("SELECT * FROM games WHERE game_code = ? LIMIT 1")
        .bind(&game_code)
        .fetch_optional(&state.db)
        .await?;

    if game.is_none() {
        return Ok(Redirect::to("/").into_response());
    }

    let sid = utils::session_id(&session)?;
    let user_auth = utils::find_user(&state, &sid).await?;

    if user_auth.is_some() {
        return Ok(Redirect::to(&format!("/games/{}", game_code)).into_response());
    }

    return Ok(Html(JoinTemplate { game_code }).into_response());
}

#[derive(Debug, Deserialize)]
struct PostGame {
    template: u64,
}

async fn post_game(
    session: Session,
    State(state): State<AppState>,
    Form(body): Form<PostGame>,
) -> Result<Response> {
    let sid = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &sid).await?.split();

    let game_template: Option<GameTemplate> = sqlx::query_as(
        "SELECT * FROM game_templates WHERE game_template_id = ? AND user_id = ? LIMIT 1",
    )
    .bind(&body.template)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game_template) = game_template else {
        return Ok((StatusCode::BAD_REQUEST, "Template not found").into_response());
    };

    let game_item_templates: Vec<GameItemTemplate> =
        sqlx::query_as("SELECT * FROM game_item_templates WHERE game_template_id = ?")
            .bind(&game_template.game_template_id)
            .fetch_all(&state.db)
            .await?;

    const GAME_CODE_CHARS: [char; 16] = [
        '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'a', 'b', 'c', 'd', 'e', 'f',
    ];
    let game_code = nanoid!(6, &GAME_CODE_CHARS);

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_secs();

    sqlx::query("INSERT INTO games (user_id, game_code, status, created_at, active_at, name, reward_message, total_reward_message, is_locked) VALUES (?, ?, ?, ?, ?, ?, ?, ?, false)")
        .bind(&user.user_id)
        .bind(&game_code)
        .bind(GAME_STATUS_ACTIVE)
        .bind(&now)
        .bind(&now)
        .bind(&game_template.name)
        .bind(&game_template.reward_message)
        .bind(&game_template.total_reward_message)
        .execute(&state.db)
        .await?;

    let game: Game = sqlx::query_as("SELECT * FROM games WHERE game_code = ?")
        .bind(&game_code)
        .fetch_one(&state.db)
        .await?;

    if !game_item_templates.is_empty() {
        let query = format!(
            "INSERT INTO game_items (game_code, name, image, enabled) VALUES {}",
            game_item_templates
                .iter()
                .map(|_| "(?, ?, ?, ?)")
                .collect::<Vec<&'static str>>()
                .join(",")
        );

        let mut q = sqlx::query(&query);

        for game_item_template in game_item_templates {
            q = q
                .bind(&game.game_code)
                .bind(game_item_template.name)
                .bind(game_item_template.image)
                .bind(game_item_template.start_enabled);
        }

        q.execute(&state.db).await?;
    }

    return Ok(Redirect::to(&format!("/games/{}", game.game_code)).into_response());
}

#[derive(Template, Clone)]
#[template(path = "games.html")]
struct GamesTemplate {
    user: User,
    games_joined: Vec<GameWithJoinedSummary>,
    games_hosted: Vec<GameWithHostedSummary>,
}

async fn games(session: Session, State(state): State<AppState>) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    let games_joined: Vec<GameWithJoinedSummary> = sqlx::query_as(
        r#"
SELECT *
FROM games
LEFT OUTER JOIN (
	SELECT game_players.game_code AS gp_game_code, COUNT(*) AS players_count
	FROM game_players
	GROUP BY gp_game_code
) AS players_counts ON players_counts.gp_game_code = games.game_code
LEFT OUTER JOIN (
	SELECT game_winners.game_code AS gw_game_code, COUNT(*) AS winners_count, MAX(points) AS winning_points
	FROM game_winners
		INNER JOIN game_players ON game_players.game_player_id = game_winners.game_player_id
	GROUP BY gw_game_code
) AS winners_counts ON winners_counts.gw_game_code = games.game_code
LEFT OUTER JOIN (
	SELECT game_winners.game_code AS gw_game_code2, COUNT(*) > 0 AS is_winner
    FROM game_winners
		INNER JOIN game_players ON game_players.game_player_id = game_winners.game_player_id
	WHERE game_players.user_id = ?
    GROUP BY gw_game_code2
) AS is_winners ON is_winners.gw_game_code2 = games.game_code
WHERE games.game_code IN (
	SELECT game_code
    FROM game_players
    WHERE user_id = ?
)
        "#,
    )
    .bind(&user.user_id)
    .bind(&user.user_id)
    .fetch_all(&state.db)
    .await?;

    let games_hosted: Vec<GameWithHostedSummary> = sqlx::query_as(
        r#"
SELECT *
FROM games
LEFT OUTER JOIN (
	SELECT game_players.game_code AS gp_game_code, COUNT(*) AS players_count
	FROM game_players
	GROUP BY gp_game_code
) AS players_counts ON players_counts.gp_game_code = games.game_code
LEFT OUTER JOIN (
	SELECT game_winners.game_code AS gw_game_code, COUNT(*) AS winners_count, MAX(points) AS winning_points
	FROM game_winners
		INNER JOIN game_players ON game_players.game_player_id = game_winners.game_player_id
	GROUP BY gw_game_code
) AS winners_counts ON winners_counts.gw_game_code = games.game_code
WHERE games.user_id = ?
        "#,
    )
    .bind(&user.user_id)
    .fetch_all(&state.db)
    .await?;

    return Ok(Html(GamesTemplate {
        user,
        games_joined,
        games_hosted,
    })
    .into_response());
}

#[derive(Template, Clone)]
#[template(path = "game-as-player.html")]
struct GameAsPlayerTemplate {
    game: Game,
    items: Vec<GameItem>,
    user: User,
    player: GamePlayer,
    img_base_uri: String,
}

#[derive(Template, Clone)]
#[template(path = "game-as-host.html")]
struct GameAsHostTemplate {
    game: Game,
    items: Vec<GameItem>,
    user: User,
    img_base_uri: String,
}

async fn game(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let user_auth = utils::find_user(&state, &session_id).await?;

    let Some(user_auth) = user_auth else {
        return Ok(Redirect::to(&format!("/join?code={game_code}")).into_response());
    };
    let (user, _) = user_auth.split();

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as("SELECT * FROM games WHERE game_code = ? LIMIT 1")
        .bind(&game_code)
        .fetch_optional(&state.db)
        .await?;

    let Some(game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    let items: Vec<GameItem> = sqlx::query_as("SELECT * FROM game_items WHERE game_code = ?")
        .bind(&game_code)
        .fetch_all(&state.db)
        .await?;

    if game.status != GAME_STATUS_ACTIVE {
        // TODO: Include all users and winners if host (?)
        // TODO: Include winning points and whether user won or lost if not host (?)

        if game.user_id == user.user_id {
            return Ok(Html(FinishedGameAsHostTemplate {
                img_base_uri: state.cfg.r2_bucket_public_url.clone(),
                game,
                items,
                user,
            })
            .into_response());
        }

        let player: Option<GamePlayer> =
            sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ?")
                .bind(&game_code)
                .bind(&user.user_id)
                .fetch_optional(&state.db)
                .await?;

        let Some(player) = player else {
            return Ok(Redirect::to("/").into_response());
        };

        compile_error!();
    }

    if game.user_id == user.user_id {
        return Ok(Html(GameAsHostTemplate {
            img_base_uri: state.cfg.r2_bucket_public_url.clone(),
            game,
            items,
            user,
        })
        .into_response());
    }

    let player: Option<GamePlayer> =
        sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ?")
            .bind(&game_code)
            .bind(&user.user_id)
            .fetch_optional(&state.db)
            .await?;

    let player = if let Some(player) = player {
        player
    } else {
        sqlx::query("INSERT INTO game_players (game_code, user_id, points) VALUES (?, ?, 0)")
            .bind(&game_code)
            .bind(&user.user_id)
            .execute(&state.db)
            .await?;

        sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ?")
            .bind(&game_code)
            .bind(&user.user_id)
            .fetch_one(&state.db)
            .await?
    };

    return Ok(Html(GameAsPlayerTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        items,
        user,
        player,
    })
    .into_response());
}

#[derive(Template, Clone)]
#[template(path = "game-as-host-board.html")]
struct GameAsHostBoardTemplate {
    game: Game,
    items: Vec<GameItem>,
    user: User,
    img_base_uri: String,
}

async fn game_x_lock(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    if !game.is_locked {
        sqlx::query("UPDATE games SET is_locked = true WHERE game_code = ?")
            .bind(&game_code)
            .execute(&state.db)
            .await?;
        game.is_locked = true;
    }

    let items: Vec<GameItem> = sqlx::query_as("SELECT * FROM game_items WHERE game_code = ?")
        .bind(&game_code)
        .fetch_all(&state.db)
        .await?;

    return Ok(Html(GameAsHostBoardTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        items,
        user,
    })
    .into_response());
}

async fn game_x_unlock(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    if game.is_locked {
        sqlx::query("UPDATE games SET is_locked = false WHERE game_code = ?")
            .bind(&game_code)
            .execute(&state.db)
            .await?;
        game.is_locked = false;
    }

    let items: Vec<GameItem> = sqlx::query_as("SELECT * FROM game_items WHERE game_code = ?")
        .bind(&game_code)
        .fetch_all(&state.db)
        .await?;

    return Ok(Html(GameAsHostBoardTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        items,
        user,
    })
    .into_response());
}

async fn game_x_choose_item(
    Path((game_code, game_item_id)): Path<(String, String)>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    let game_item: Option<GameItem> =
        sqlx::query_as("SELECT * FROM game_items WHERE game_code = ? AND game_item_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&game_item_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(game_item) = game_item else {
        return Ok(Redirect::to("/").into_response());
    };

    if !game_item.enabled {
        return Err(anyhow::anyhow!("Item is disabled"))?;
    }

    sqlx::query("INSERT INTO game_item_outcomes (game_code, item_id) VALUES (?, ?)")
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

    let outcome: GameItemOutcome = sqlx::query_as(
        "SELECT * FROM game_item_outcomes WHERE item_id = ? ORDER BY outcome_id DESC LIMIT 1",
    )
    .bind(&game_item_id)
    .fetch_one(&state.db)
    .await?;

    sqlx::query("UPDATE player_guesses SET outcome_id = ? WHERE game_code = ? AND item_id = ? AND outcome_id IS NULL")
        .bind(&outcome.outcome_id)
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

    sqlx::query("UPDATE game_items SET enabled = false WHERE game_code = ? AND game_item_id = ?")
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

    let items: Vec<GameItem> = sqlx::query_as("SELECT * FROM game_items WHERE game_code = ?")
        .bind(&game_code)
        .fetch_all(&state.db)
        .await?;

    if game.is_locked {
        sqlx::query("UPDATE games SET is_locked = false WHERE game_code = ?")
            .bind(&game_code)
            .execute(&state.db)
            .await?;
        game.is_locked = false;
    }

    return Ok(Html(GameAsHostBoardTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        items,
        user,
    })
    .into_response());
}

#[derive(Template, Clone)]
#[template(path = "game-as-host-item.html")]
struct GameAsHostItemTemplate {
    game: Game,
    item: GameItem,
    img_base_uri: String,
}

async fn game_x_enable_item(
    Path((game_code, game_item_id)): Path<(String, String)>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    let game_item: Option<GameItem> =
        sqlx::query_as("SELECT * FROM game_items WHERE game_code = ? AND game_item_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&game_item_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(mut game_item) = game_item else {
        return Ok(Redirect::to("/").into_response());
    };

    if !game_item.enabled {
        sqlx::query(
            "UPDATE game_items SET enabled = true WHERE game_code = ? AND game_item_id = ?",
        )
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

        game_item.enabled = true;
    }

    return Ok(Html(GameAsHostItemTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        item: game_item,
    })
    .into_response());
}

async fn game_x_disable_item(
    Path((game_code, game_item_id)): Path<(String, String)>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = 'ACTIVE' LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .fetch_optional(&state.db)
    .await?;

    let Some(game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    let game_item: Option<GameItem> =
        sqlx::query_as("SELECT * FROM game_items WHERE game_code = ? AND game_item_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&game_item_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(mut game_item) = game_item else {
        return Ok(Redirect::to("/").into_response());
    };

    if game_item.enabled {
        sqlx::query(
            "UPDATE game_items SET enabled = false WHERE game_code = ? AND game_item_id = ?",
        )
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

        game_item.enabled = false;
    }

    return Ok(Html(GameAsHostItemTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        item: game_item,
    })
    .into_response());
}

async fn finish_game(
    Path(game_code): Path<String>,
    session: Session,
    State(state): State<AppState>,
) -> Result<Response> {
    let session_id = utils::session_id(&session)?;
    let (user, _) = utils::require_user(&state, &session_id).await?.split();

    if game_code.trim().is_empty() {
        return Ok(Redirect::to("/").into_response());
    }
    let game_code = game_code.to_lowercase();

    let game: Option<Game> = sqlx::query_as(
        "SELECT * FROM games WHERE game_code = ? AND user_id = ? AND status = ? LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .bind(GAME_STATUS_ACTIVE)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    if game.status != GAME_STATUS_ACTIVE {
        return Ok(Redirect::to("/").into_response());
    }

    sqlx::query("UPDATE games SET status = ? WHERE game_code = ?")
        .bind(GAME_STATUS_FINISHED)
        .bind(&game.game_code)
        .execute(&state.db)
        .await?;
    game.status = GAME_STATUS_FINISHED.to_string();

    let players: Vec<(u64, i32)> = sqlx::query_as(
        r#"
SELECT game_players.game_player_id, COUNT(*) AS points
FROM player_guesses
    INNER JOIN game_item_outcomes ON player_guesses.outcome_id = game_item_outcomes.outcome_id
    INNER JOIN game_players ON player_guesses.player_id = game_players.game_player_id
WHERE
	player_guesses.game_code = ? AND
    game_item_outcomes.game_code = ? AND
	game_players.game_code = ? AND
	player_guesses.item_id = game_item_outcomes.item_id
GROUP BY player_guesses.player_id
ORDER BY points DESC
"#,
    )
    .bind(&game_code)
    .bind(&game_code)
    .bind(&game_code)
    .fetch_all(&state.db)
    .await?;

    if !players.is_empty() {
        let max_points = players[0].1;

        let mut winners = vec![];

        for (game_player_id, points) in players {
            if points < max_points {
                break;
            }

            winners.push(game_player_id);
        }

        if !winners.is_empty() {
            let values = winners
                .iter()
                .map(|_| "(?, ?)")
                .collect::<Vec<_>>()
                .join(", ");

            let q = format!("INSERT INTO game_winners (game_player_id, game_code) VALUES {values}");
            let mut query = sqlx::query(&q);

            for game_player_id in winners {
                query = query.bind(game_player_id).bind(&game_code);
            }

            query.execute(&state.db).await?;
        }
    }

    return Ok(Redirect::to(&format!("/games/{game_code}")).into_response());
}
