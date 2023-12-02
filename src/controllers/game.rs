use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

use super::*;

use crate::{
    models::{
        Game, GameItem, GameItemOutcome, GameItemTemplate, GameItemWithGuessCount, GamePlayer,
        GameTemplate, GameWithHostedSummary, GameWithJoinedSummary, PlayerGuess, User,
        GAME_STATUS_ACTIVE, GAME_STATUS_FINISHED,
    },
    prelude::*,
    pubsub::{HostAction, HostActionType, PlayerAction, PlayerActionType},
    GameBroadcast,
};

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::{
        sse::{Event, Sse},
        IntoResponse, Redirect, Response,
    },
    routing::{get, post, put},
    Form, Router,
};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;
use tower_sessions::Session;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    return router
        .route("/join", get(join))
        .route("/games", get(games).post(post_game))
        .route("/games/:game_code", get(game))
        .route("/games/:game_code/finish", post(finish_game))
        .route("/games/:game_code/x/board", get(game_x_board))
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
        )
        .route(
            "/games/:game_code/items/:game_item_id/x/guess",
            put(game_x_guess_item),
        )
        .route("/games/:game_code/sse/host", get(host_sse))
        .route("/games/:game_code/sse/player", get(player_sse));
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

    let game_code = {
        let mut game_code;

        // Keep searching until unique code is found or request times out
        loop {
            const GAME_CODE_LENGTH: usize = 6;
            const GAME_CODE_CHARS: [char; 16] = [
                '1', '2', '3', '4', '5', '6', '7', '8', '9', '0', 'a', 'b', 'c', 'd', 'e', 'f',
            ];

            game_code = nanoid!(GAME_CODE_LENGTH, &GAME_CODE_CHARS);

            let existing: Option<Game> = sqlx::query_as("SELECT * FROM games WHERE game_code = ?")
                .bind(&game_code)
                .fetch_optional(&state.db)
                .await?;

            if existing.is_none() {
                break;
            }
        }

        game_code
    };

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
	SELECT game_code AS gp2_game_code, points
	FROM game_players
    WHERE user_id = ?
) AS points ON points.gp2_game_code = games.game_code
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
LEFT OUTER JOIN (
    SELECT user_id as u_user_id, username AS host
    FROM users
) AS hosts ON hosts.u_user_id = games.user_id
LEFT OUTER JOIN (
    SELECT game_code as gio_game_code, COUNT(*) AS total_drops
    FROM game_item_outcomes
    GROUP BY gio_game_code
) AS total_drops ON total_drops.gio_game_code = games.game_code
WHERE games.game_code IN (
	SELECT game_code
    FROM game_players
    WHERE user_id = ?
)
ORDER BY status ASC, created_at DESC
        "#,
    )
    .bind(&user.user_id)
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
LEFT OUTER JOIN (
    SELECT game_code as gio_game_code, COUNT(*) AS total_drops
    FROM game_item_outcomes
    GROUP BY gio_game_code
) AS total_drops ON total_drops.gio_game_code = games.game_code
WHERE games.user_id = ?
ORDER BY status ASC, created_at DESC
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
    guess: Option<PlayerGuess>,
    items: Vec<GameItemWithGuessCount>,
    user: User,
    player: GamePlayer,
    img_base_uri: String,
}

#[derive(Template, Clone)]
#[template(path = "game-as-host.html")]
struct GameAsHostTemplate {
    game: Game,
    items: Vec<GameItemWithGuessCount>,
    user: User,
    img_base_uri: String,
}

#[derive(Template, Clone)]
#[template(path = "finished-game-as-player.html")]
struct FinishedGameAsPlayerTemplate {
    game: Game,
    items: Vec<GameItemWithGuessCount>,
    user: User,
    player: GamePlayer,
    img_base_uri: String,
}

#[derive(Template, Clone)]
#[template(path = "finished-game-as-host.html")]
struct FinishedGameAsHostTemplate {
    game: Game,
    items: Vec<GameItemWithGuessCount>,
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

    let items: Vec<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
    )
    .bind(&game_code)
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

        return Ok(Html(FinishedGameAsPlayerTemplate {
            img_base_uri: state.cfg.r2_bucket_public_url.clone(),
            game,
            items,
            user,
            player,
        })
        .into_response());
    }

    {
        let broadcasts = &mut *state.game_broadcasts.write().unwrap();

        let _entry = broadcasts.entry(game_code.clone()).or_insert_with(|| {
            let (host_tx, _host_rx) = broadcast::channel(16);
            let (players_tx, _players_rx) = broadcast::channel(16);

            return GameBroadcast {
                to_host: host_tx,
                to_players: players_tx,
            };
        });
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

    let (player, guess) = if let Some(player) = player {
        let guess: Option<PlayerGuess> = sqlx::query_as("SELECT * FROM player_guesses WHERE game_code = ? AND player_id = ? AND outcome_id IS NULL LIMIT 1")
            .bind(&game_code)
            .bind(&player.game_player_id)
            .fetch_optional(&state.db)
            .await?;

        (player, guess)
    } else {
        sqlx::query("INSERT INTO game_players (game_code, user_id, points) VALUES (?, ?, 0)")
            .bind(&game_code)
            .bind(&user.user_id)
            .execute(&state.db)
            .await?;

        state
            .pubsub
            .player_actions
            .publish(PlayerAction {
                game_code: game_code.clone(),
                user_id: user.user_id.clone(),
                typ: PlayerActionType::Join,
            })
            .await?;

        (
            sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ?")
                .bind(&game_code)
                .bind(&user.user_id)
                .fetch_one(&state.db)
                .await?,
            None,
        )
    };

    return Ok(Html(GameAsPlayerTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        guess,
        items,
        user,
        player,
    })
    .into_response());
}

async fn game_x_board(
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

    let game: Option<Game> =
        sqlx::query_as("SELECT * FROM games WHERE game_code = ? AND status = 'ACTIVE' LIMIT 1")
            .bind(&game_code)
            .fetch_optional(&state.db)
            .await?;

    let Some(game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    if game.user_id == user.user_id {
        let items: Vec<GameItemWithGuessCount> = sqlx::query_as(
            r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
        )
        .bind(&game_code)
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

    let game_player: Option<GamePlayer> =
        sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&user.user_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(game_player) = game_player else {
        return Ok(Redirect::to("/").into_response());
    };

    let guess: Option<PlayerGuess> = sqlx::query_as("SELECT * FROM player_guesses WHERE game_code = ? AND player_id = ? AND outcome_id IS NULL LIMIT 1")
        .bind(&game_code)
        .bind(&game_player.game_player_id)
        .fetch_optional(&state.db)
        .await?;

    let items = sqlx::query_as("SELECT * FROM game_items WHERE game_code = ?")
        .bind(&game_code)
        .fetch_all(&state.db)
        .await?;

    return Ok(Html(GameAsPlayerBoardTemplate {
        game,
        guess,
        user,
        items,
        player: game_player,
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
    })
    .into_response());
}

#[derive(Template, Clone)]
#[template(path = "game-as-host-board.html")]
struct GameAsHostBoardTemplate {
    game: Game,
    items: Vec<GameItemWithGuessCount>,
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

    state
        .pubsub
        .host_actions
        .publish(HostAction {
            game_code: game_code.clone(),
            typ: HostActionType::Lock,
        })
        .await?;

    let items: Vec<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
    )
    .bind(&game_code)
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

    state
        .pubsub
        .host_actions
        .publish(HostAction {
            game_code: game_code.clone(),
            typ: HostActionType::Unlock,
        })
        .await?;

    let items: Vec<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
    )
    .bind(&game_code)
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
    Path((game_code, game_item_id)): Path<(String, u64)>,
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

    let correct_guesses: Vec<PlayerGuess> = sqlx::query_as(
        "SELECT * FROM player_guesses WHERE game_code = ? AND item_id = ? AND outcome_id IS NULL",
    )
    .bind(&game_code)
    .bind(&game_item_id)
    .fetch_all(&state.db)
    .await?;

    if !correct_guesses.is_empty() {
        let correct_guesses: HashSet<u64> =
            correct_guesses.into_iter().map(|g| g.player_id).collect();

        let values = correct_guesses
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(", ");

        let q = format!(
            "UPDATE game_players SET points = points + 1 WHERE game_player_id IN ({values})"
        );

        let mut query = sqlx::query(&q);

        for id in correct_guesses {
            query = query.bind(id);
        }

        query.execute(&state.db).await?;
    }

    sqlx::query(
        "UPDATE player_guesses SET outcome_id = ? WHERE game_code = ? AND outcome_id IS NULL",
    )
    .bind(&outcome.outcome_id)
    .bind(&game_code)
    .execute(&state.db)
    .await?;

    sqlx::query("UPDATE game_items SET enabled = false WHERE game_code = ? AND game_item_id = ?")
        .bind(&game_code)
        .bind(&game_item_id)
        .execute(&state.db)
        .await?;

    if game.is_locked {
        sqlx::query("UPDATE games SET is_locked = false WHERE game_code = ?")
            .bind(&game_code)
            .execute(&state.db)
            .await?;
        game.is_locked = false;
    }

    state
        .pubsub
        .host_actions
        .publish(HostAction {
            game_code: game_code.clone(),
            typ: HostActionType::Choose {
                item_id: game_item_id.clone(),
            },
        })
        .await?;

    let items: Vec<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ?
            "#,
    )
    .bind(&game_code)
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

#[derive(Template, Clone)]
#[template(path = "game-as-player-board.html")]
struct GameAsPlayerBoardTemplate {
    game: Game,
    guess: Option<PlayerGuess>,
    items: Vec<GameItem>,
    user: User,
    player: GamePlayer,
    img_base_uri: String,
}

async fn game_x_guess_item(
    Path((game_code, game_item_id)): Path<(String, u64)>,
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
        "SELECT * FROM games WHERE game_code = ? AND user_id != ? AND status = 'ACTIVE' LIMIT 1",
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

    let Some(game_item) = game_item else {
        return Ok(Redirect::to("/").into_response());
    };

    let game_player: Option<GamePlayer> =
        sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&user.user_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(game_player) = game_player else {
        return Ok(Redirect::to("/").into_response());
    };

    let guess: Option<PlayerGuess> = sqlx::query_as("SELECT * FROM player_guesses WHERE game_code = ? AND player_id = ? AND outcome_id IS NULL LIMIT 1")
        .bind(&game_code)
        .bind(&game_player.game_player_id)
        .fetch_optional(&state.db)
        .await?;

    let guess = if let Some(mut guess) = guess {
        sqlx::query("UPDATE player_guesses SET item_id = ? WHERE game_code = ? AND player_id = ? AND outcome_id IS NULL LIMIT 1")
            .bind(&game_item.game_item_id)
            .bind(&game_code)
            .bind(&game_player.game_player_id)
            .execute(&state.db)
            .await?;

        state
            .pubsub
            .player_actions
            .publish(PlayerAction {
                game_code: game_code.clone(),
                user_id: user.user_id.clone(),
                typ: PlayerActionType::ChangeGuess {
                    from_item_id: guess.item_id.clone(),
                    to_item_id: game_item.game_item_id.clone(),
                },
            })
            .await?;

        guess.item_id = game_item.game_item_id;

        guess
    } else {
        sqlx::query("INSERT INTO player_guesses (game_code, player_id, item_id, outcome_id) VALUES (?, ?, ?, ?)")
            .bind(&game_code)
            .bind(&game_player.game_player_id)
            .bind(&game_item.game_item_id)
            .bind(None as Option<u64>)
            .execute(&state.db)
            .await?;

        state
            .pubsub
            .player_actions
            .publish(PlayerAction {
                game_code: game_code.clone(),
                user_id: user.user_id.clone(),
                typ: PlayerActionType::Guess {
                    item_id: game_item.game_item_id.clone(),
                },
            })
            .await?;

        let guess: PlayerGuess = sqlx::query_as("SELECT * FROM player_guesses WHERE game_code = ? AND player_id = ? AND item_id = ? AND outcome_id IS NULL LIMIT 1")
            .bind(&game_code)
            .bind(&game_player.game_player_id)
            .bind(&game_item.game_item_id)
            .fetch_one(&state.db)
            .await?;

        guess
    };

    let items = sqlx::query_as("SELECT * FROM game_items WHERE game_code = ?")
        .bind(&game_code)
        .fetch_all(&state.db)
        .await?;

    return Ok(Html(GameAsPlayerBoardTemplate {
        game,
        guess: Some(guess),
        user,
        items,
        player: game_player,
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
    })
    .into_response());
}

#[derive(Template, Clone)]
#[template(path = "game-as-host-item.html")]
struct GameAsHostItemTemplate {
    game: Game,
    item: GameItemWithGuessCount,
    img_base_uri: String,
}

async fn game_x_enable_item(
    Path((game_code, game_item_id)): Path<(String, u64)>,
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

    let game_item: Option<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
        HAVING item_id = ?
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ? AND
    game_item_id = ?
LIMIT 1
            "#,
    )
    .bind(&game_code)
    .bind(&game_item_id)
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

        state
            .pubsub
            .host_actions
            .publish(HostAction {
                game_code: game_code.clone(),
                typ: HostActionType::Enable {
                    item_id: game_item_id.clone(),
                },
            })
            .await?;
    }

    return Ok(Html(GameAsHostItemTemplate {
        img_base_uri: state.cfg.r2_bucket_public_url.clone(),
        game,
        item: game_item,
    })
    .into_response());
}

async fn game_x_disable_item(
    Path((game_code, game_item_id)): Path<(String, u64)>,
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

    let game_item: Option<GameItemWithGuessCount> = sqlx::query_as(
        r#"
SELECT *
FROM game_items
    LEFT OUTER JOIN (
        SELECT item_id, COUNT(*) AS guess_count
        FROM player_guesses
        WHERE
            game_code = ? AND
            outcome_id IS NULL
        GROUP BY item_id
        HAVING item_id = ?
    ) AS guess_counts ON guess_counts.item_id = game_items.game_item_id
WHERE
    game_code = ? AND
    game_item_id = ?
LIMIT 1
            "#,
    )
    .bind(&game_code)
    .bind(&game_item_id)
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

        state
            .pubsub
            .host_actions
            .publish(HostAction {
                game_code: game_code.clone(),
                typ: HostActionType::Disable {
                    item_id: game_item_id.clone(),
                },
            })
            .await?;
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

    let winners: Vec<GamePlayer> = sqlx::query_as(
        r#"
SELECT *
FROM game_players
WHERE
    game_players.game_code = ? AND
    points != 0 AND
    points = (
        SELECT MAX(points)
        FROM game_players
        WHERE game_players.game_code = ?
    )
"#,
    )
    .bind(&game_code)
    .bind(&game_code)
    .fetch_all(&state.db)
    .await?;

    if !winners.is_empty() {
        let values = winners
            .iter()
            .map(|_| "(?, ?)")
            .collect::<Vec<_>>()
            .join(", ");

        let q = format!("INSERT INTO game_winners (game_player_id, game_code) VALUES {values}");
        let mut query = sqlx::query(&q);

        for winner in winners {
            query = query.bind(winner.game_player_id).bind(&game_code);
        }

        query.execute(&state.db).await?;
    }

    return Ok(Redirect::to(&format!("/games/{game_code}")).into_response());
}

async fn host_sse(
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

    let Some(game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    if game.status != GAME_STATUS_ACTIVE {
        return Ok(Redirect::to("/").into_response());
    }

    let guard = state.game_broadcasts.read().unwrap();
    let game_broadcasts = &*guard;

    let broadcast = match game_broadcasts.get(&game_code) {
        Some(broadcast) => broadcast.clone(),
        None => {
            drop(guard);

            let broadcasts = &mut *state.game_broadcasts.write().unwrap();

            broadcasts
                .entry(game_code.clone())
                .or_insert_with(|| {
                    let (host_tx, _host_rx) = broadcast::channel(16);
                    let (players_tx, _players_rx) = broadcast::channel(16);

                    return GameBroadcast {
                        to_host: host_tx,
                        to_players: players_tx,
                    };
                })
                .clone()
        }
    };

    let rx = broadcast.to_host.subscribe();

    let stream = BroadcastStream::new(rx).map(move |event| -> Result<Event> {
        // let name = match &event?.typ {
        //     PlayerActionType::Join => "join",
        //     PlayerActionType::Guess { .. } => "guess",
        //     PlayerActionType::ChangeGuess { .. } => "change_guess",
        // };
        let name = "player_action";

        return Ok(Event::default().event(name).data(name));
    });

    return Ok(Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(1))
                .text("keep-alive-text"),
        )
        .into_response());
}

async fn player_sse(
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
        "SELECT * FROM games WHERE game_code = ? AND user_id != ? AND status = ? LIMIT 1",
    )
    .bind(&game_code)
    .bind(&user.user_id)
    .bind(GAME_STATUS_ACTIVE)
    .fetch_optional(&state.db)
    .await?;

    let Some(game) = game else {
        return Ok(Redirect::to("/").into_response());
    };

    if game.status != GAME_STATUS_ACTIVE {
        return Ok(Redirect::to("/").into_response());
    }

    let game_player: Option<GamePlayer> =
        sqlx::query_as("SELECT * FROM game_players WHERE game_code = ? AND user_id = ? LIMIT 1")
            .bind(&game_code)
            .bind(&user.user_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(_) = game_player else {
        return Ok(Redirect::to("/").into_response());
    };

    let guard = state.game_broadcasts.read().unwrap();
    let game_broadcasts = &*guard;

    let broadcast = match game_broadcasts.get(&game_code) {
        Some(broadcast) => broadcast.clone(),
        None => {
            drop(guard);

            let broadcasts = &mut *state.game_broadcasts.write().unwrap();

            broadcasts
                .entry(game_code.clone())
                .or_insert_with(|| {
                    let (host_tx, _host_rx) = broadcast::channel(16);
                    let (players_tx, _players_rx) = broadcast::channel(16);

                    return GameBroadcast {
                        to_host: host_tx,
                        to_players: players_tx,
                    };
                })
                .clone()
        }
    };

    let rx = broadcast.to_players.subscribe();

    let stream = BroadcastStream::new(rx).map(|event| -> Result<Event> {
        // let name = match &event?.typ {
        //     HostActionType::Lock => "lock",
        //     HostActionType::Unlock => "unlock",
        //     HostActionType::Choose { .. } => "choose",
        //     HostActionType::Enable { .. } => "enable",
        //     HostActionType::Disable { .. } => "disable",
        // };
        let name = "host_action";

        return Ok(Event::default().event(name).data(name));

        // return tokio::runtime::Runtime::new()?.block_on(async move {
        //     let host_action = event?;
        //     match &host_action.typ {
        //         HostActionType::Lock => {
        //             // let res = game_x_lock(
        //             //     Path((game_code.clone())),
        //             //     session.clone(),
        //             //     State(state.clone()),
        //             // )
        //             // .await?;
        //         }

        //         _ => compile_error!(),
        //     }

        //     return Ok(Event::default().data(format!(
        //         "<p>{:#?}</p>",
        //         serde_json::to_string_pretty(&host_action),
        //     )));
        // });
    });

    return Ok(Sse::new(stream)
        .keep_alive(
            axum::response::sse::KeepAlive::new()
                .interval(Duration::from_secs(1))
                .text("keep-alive-text"),
        )
        .into_response());
}
