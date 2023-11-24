use std::time::SystemTime;

use super::*;

use crate::{
    models::{
        Game, GameItem, GameItemTemplate, GamePlayer, GameTemplate, GameWithHostedSummary,
        GameWithJoinedSummary, User, GAME_STATUS_ACTIVE,
    },
    prelude::*,
};

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Form, Router,
};
use serde::Deserialize;
use tower_sessions::Session;

pub fn add_routes(router: Router<AppState>) -> Router<AppState> {
    return router
        .route("/join", get(join))
        .route("/games", get(games).post(post_game))
        .route("/games/:game_code", get(game));
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

    sqlx::query("INSERT INTO games (user_id, game_code, status, created_at, active_at, name, reward_message, total_reward_message) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
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
