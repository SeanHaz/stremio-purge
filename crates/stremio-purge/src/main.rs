use axum::{
    Json, Router, 
    extract::{ Request, State },
    http::{ StatusCode },
    middleware::{ self, Next },
    response::{ Html, IntoResponse, Redirect },
    routing::{ get, post }, 
};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use axum_embed::ServeEmbed;
use rust_embed::RustEmbed;
use tokio_util::sync::CancellationToken;
use log::info;
use std::env;

use db::repository::upsert_user_on_login;
use serde::{Deserialize, Serialize};
use std::{sync::Arc};
use reqwest::Client;
use std::net::SocketAddr;
use tower_sessions::Session;
use tower_sessions_cookie_store::{ CookieSessionConfig, CookieSessionManagerLayer, Key, SameSite };
use time::Duration;
use db::models::{ SYNC_TV, SYNC_SERIES, SYNC_ALL, SYNC_MOVIES };

use stremio_api::get_auth;

#[derive(RustEmbed, Clone)]
#[folder = "../../static/"]
struct Assets;

#[derive(Clone)]
struct AppState {
    http_client: Client,
    pool: sqlx::sqlite::SqlitePool,
    tasks: Arc<RwLock<HashMap<String, (JoinHandle<()>, CancellationToken)>>>,
}

#[derive(Deserialize, Serialize, Clone)]
struct Config {
    all: bool,
    movies: bool,
    series: bool,
    tv: bool,
}

#[derive(Serialize)]
struct ApiResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}


#[derive(Serialize)]
struct ConfigResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    config: Option<Config>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: Option<String>,
    password: Option<String>,
    auth_key: Option<String>,
}

#[derive(Serialize)]
struct LoginResponse {
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

async fn require_auth(session: Session, request: Request<axum::body::Body>, next: Next) -> impl IntoResponse {
    if let Ok(Some(_uid)) = session.get::<String>("id").await {
        return next.run(request).await;
    }
    info!("redirecting to login");
    return Redirect::to("/").into_response();
}

#[axum::debug_handler]
async fn get_config(
    session: Session,
    State(state): State<Arc<AppState>>,
) -> Json<ConfigResponse> { 
    let uid = match session.get::<String>("id").await {
        Ok(Some(id)) => id,
        _ => return Json(ConfigResponse {
            success: false,
            config: None,
            error: Some("Not logged in".to_string()),
        }),
    };

    match db::find_user_by_id(&state.pool, &uid).await {
        Ok(Some(user)) => {
            let config = Config {
                all:     user.config_mask == SYNC_ALL,
                movies:  (user.config_mask & SYNC_MOVIES) != 0,
                series:  (user.config_mask & SYNC_SERIES) != 0,
                tv:      (user.config_mask & SYNC_TV) != 0,
            };
            Json(ConfigResponse {
                success: true,
                config: Some(config),
                error: None,
            })
        }
        _ => Json(ConfigResponse {
            success: false,
            config: None,
            error: Some("User not found".to_string()),
        }),
    }
}



#[axum::debug_handler]
async fn login_handler(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(payload): Json<LoginRequest>,
) -> Json<LoginResponse> {
    if let Some(auth_key) = &payload.auth_key {
        match stremio_api::get_user_id(&state.http_client, &auth_key).await {
            Ok(id) =>  {
                
                let _ = upsert_user_on_login(&state.pool, &id, &auth_key).await;
                
                if let Err(_) = session.insert("id", &id).await {
                    return Json(LoginResponse {
                        success: false,
                        auth_key: None,
                        error: Some(String::from("Failed to save update session cookie")),
                    });
                }

                return Json(LoginResponse {
                    success: true,
                    auth_key: Some(id),
                    error: None,
                });
            },
            Err(e) => {
                return Json(LoginResponse {
                    success: false,
                    auth_key: None,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    if let (Some(email), Some(pass)) = (&payload.email, &payload.password) {
        match get_auth(&state.http_client, &email, &pass).await {
            Ok(key) => {
                let id = stremio_api::get_user_id(&state.http_client, &key)
                    .await
                    .unwrap_or_default();
                if let Err(e) = upsert_user_on_login(&state.pool, &id, &key).await {
                    return Json(LoginResponse {
                        success: false,
                        auth_key: None,
                        error: Some(e.to_string()),
                    });
                }

                if let Err(_) = session.insert("id", &id).await {
                    return Json(LoginResponse {
                        success: false,
                        auth_key: None,
                        error: Some(String::from("Failed to save update session cookie")),
                    });
                }

                return Json(LoginResponse {
                    success: true,
                    auth_key: Some(id),
                    error: None,
                });
            }
            
            Err(e) => {
                return Json(LoginResponse {
                    success: false,
                    auth_key: None,
                    error: Some(e.to_string())});
            }
        }
    }
    return Json(LoginResponse {
        success: false,
        auth_key: None,
        error: Some(String::from("No auth_key or user&pass sent"))
    });
}




#[axum::debug_handler]
async fn root_handler(session: Session) -> impl IntoResponse {
    if let Ok(Some(_uid)) = session.get::<String>("id").await {
        info!("User {} already logged in, redirecting to config", _uid);
        return Redirect::to("/config").into_response();
    }

    match Assets::get("index.html") {
        Some(content) => {
            Html(content.data).into_response()
        }
        None => {
            (StatusCode::NOT_FOUND, "Critical Error: index.html missing from binary").into_response()
        }
    }
}

async fn start_update_task(state: Arc<AppState>, uid: String, mask: i64) {
    {
        let mut tasks = state.tasks.write().await;
        if let Some((_, token)) = tasks.remove(&uid) {
            token.cancel();
        }
    }
            
    let token = CancellationToken::new();
    let pool_clone = state.pool.clone();
    let client_clone = state.http_client.clone();
    let uid_clone = uid.clone();
    let mask_clone = mask;

    let handle: JoinHandle<()> = tokio::spawn({
        let token = token.clone();
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
        async move {
            loop {
                tokio::select!{
                    _ = token.cancelled() =>{
                        break;
                    }
                    _ = interval.tick() => {}
                }
                    
                let user = match db::find_user_by_id(&pool_clone, &uid_clone).await {
                    Ok(Some(u)) => u,
                    _ => continue,
                };

                let max_timestamp = match stremio_api::update_library_flow(&client_clone, &user).await {
                    Some(ts) => ts,
                    None => continue,
                };

                let _ = db::update_timestamps(&pool_clone, mask_clone, &uid_clone, max_timestamp).await;
            }
        }});

    {
        let mut tasks = state.tasks.write().await;
        tasks.insert(uid, (handle, token));
    }
}

#[axum::debug_handler]
async fn update_handler(session: Session,State(state): State<Arc<AppState>>,Json(payload): Json<Config>) -> Json<ApiResponse> {
    let uid = match session.get::<String>("id").await {
        Ok(Some(id)) => id,
        _ => return Json(ApiResponse {
            success: false,
            error: Some("Not logged in".to_string()),
        }),
    };

    let mut mask: i64 = 0;
    if payload.all    { mask |= SYNC_ALL; }
    if payload.movies { mask |= SYNC_MOVIES; }
    if payload.series { mask |= SYNC_SERIES; }
    if payload.tv     { mask |= SYNC_TV; }

    let update_result = sqlx::query!(
        "UPDATE users SET config_mask = $1 WHERE id = $2",
        mask,
        uid
    )
    .execute(&state.pool)
        .await;
    match update_result {
        Ok(result) if result.rows_affected() > 0 => {
            
            start_update_task(state.clone(), uid, mask).await;
            Json(ApiResponse { success: true, error: None })
        },
        _ => Json(ApiResponse {
            success: false,
            error: Some("Failed to update config - updateing-config mask".to_string()),
        }),
    }
}

#[axum::debug_handler]
async fn logout_handler(session: Session) -> Json<ApiResponse> {
    // Remove the user id from session (clears the cookie on next response)
    let _ = session.remove_value("id").await;
    
    Json(ApiResponse {
        success: true,
        error: None,
    })
}

fn get_secret_key() -> Key {
    let secret = env::var("COOKIE_SECRET")
        .expect("COOKIE_SECERT env variable must be set");

    if secret.len() < 32 {
        panic!("COOKIE_SECERT must be at least 32 bytes long");
    }

    Key::from(secret.as_bytes())
}

#[tokio::main]
async fn main() {
    let client = Client::new();
    let pool = db::init_pool(env::var("DATABASE_URL")
        .expect("DATABASE_URL environment variable must be set").as_str()).await.unwrap();
    db::create_tables(&pool).await.unwrap();
    env_logger::init();
    let state = Arc::new(
        AppState {
            http_client: client,
            pool: pool,
            tasks: Arc::new(RwLock::new(HashMap::new())),
        });

    {
        let all_configs = match sqlx::query!(
            "SELECT id, config_mask FROM users"
        )
        .fetch_all(&state.pool)
        .await
        {
            Ok(rows) => rows,
            Err(e) => {
                eprintln!("Warning: Failed to load user configs on startup: {}", e);
                vec![]
            }
        };

        for row in all_configs {
            let uid: String = row.id.unwrap_or_default();
            let mask: i64 = row.config_mask;
            start_update_task(state.clone(), uid, mask).await;
        }
    }
    
    let secret_key = get_secret_key();

    let config = CookieSessionConfig::default()
        .with_name("purge_session")
        .with_expiry(tower_sessions_cookie_store::Expiry::OnInactivity(Duration::days(30)))
        .with_secure(true)
        .with_http_only(true)
        .with_same_site(SameSite::Lax)
        .with_path("/");

    let session_layer = CookieSessionManagerLayer::signed(secret_key)
        .with_config(config);

    let protected_routes = Router::new()
        .route("/config", get(|| async {
             match Assets::get("config/index.html") {
                Some(content) => Html(content.data).into_response(),
                None => (StatusCode::NOT_FOUND, "Config index missing").into_response(),
            }
        }))
        .layer(middleware::from_fn(require_auth));

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/api/login", post(login_handler))
        .route("/api/update", post(update_handler))
        .route("/api/logout", post(logout_handler))
        .route("/api/config", get(get_config))
        .merge(protected_routes)        
        .fallback_service(ServeEmbed::<Assets>::new())
        .layer(session_layer)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0],8080));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    info!("Server running at http://{}", addr);
    
    axum::serve(listener, app).await.unwrap();
}
 
