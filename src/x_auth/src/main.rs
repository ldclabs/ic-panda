use axum::{middleware, routing, Router};
use dotenvy::dotenv;
use lib_panda::{bytes32_from_base64, SigningKey};
use reqwest::ClientBuilder;
use std::{sync::Arc, time::Duration};
use structured_logger::{async_json::new_writer, get_env_level, Builder};
use tokio::signal;
use tower_http::{
    catch_panic::CatchPanicLayer,
    compression::{predicate::SizeAbove, CompressionLayer},
    cors::CorsLayer,
    timeout::TimeoutLayer,
};

mod api;
mod api_twitter;
mod context;
mod erring;

#[tokio::main]
async fn main() {
    dotenv().expect(".env file not found");
    let addr = std::env::var("SERVER_ADDR").unwrap_or("127.0.0.1:8080".to_string());

    Builder::with_level(&get_env_level().to_string())
        .with_target_writer("*", new_writer(tokio::io::stdout()))
        .init();

    let http_client = ClientBuilder::new()
        .http2_keep_alive_interval(Some(Duration::from_secs(25)))
        .http2_keep_alive_timeout(Duration::from_secs(15))
        .http2_keep_alive_while_idle(true)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(10))
        .gzip(true)
        .build()
        .unwrap();

    let secret = bytes32_from_base64(&std::env::var("CHALLENGE_SECRET").unwrap()).unwrap();
    let app_state = api::AppState {
        http_client: Arc::new(http_client),
        state_secret: std::env::var("STATE_SECRET").unwrap().into_bytes(),
        challenge_secret: SigningKey::from_bytes(&secret),
        ic_redirect_uri: std::env::var("IC_REDIRECT_URI").unwrap(),
        test_redirect_uri: std::env::var("TEST_REDIRECT_URI").unwrap(),
        local_redirect_uri: std::env::var("LOCAL_REDIRECT_URI").unwrap(),
        twitter: api::AuthConfig {
            client_id: std::env::var("X_CLIENT_ID").unwrap(),
            client_secret: std::env::var("X_CLIENT_SECRET").unwrap(),
            callback_url: std::env::var("X_CALLBACK_URL").unwrap(),
        },
    };

    let app = Router::new()
        .route("/", routing::get(api::healthz).head(api::healthz))
        .route("/healthz", routing::get(api::healthz).head(api::healthz))
        .route(
            "/idp/twitter/authorize",
            routing::get(api_twitter::authorize),
        )
        .route("/idp/twitter/callback", routing::get(api_twitter::callback))
        .layer((
            CatchPanicLayer::new(),
            TimeoutLayer::new(Duration::from_secs(10)),
            CorsLayer::very_permissive(),
            middleware::from_fn(context::middleware),
            CompressionLayer::new().compress_when(SizeAbove::new(256)),
        ))
        .with_state(app_state);
    let shutdown = shutdown_signal();

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    log::warn!(target: "server", "listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    log::warn!(target: "server", "signal received, starting graceful shutdown");
}
