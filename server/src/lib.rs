pub mod delta;
pub mod net;
pub mod protocol;
pub mod save;
pub mod sim;

pub use net::{run_game_loop, ws_handler, Server, SharedServer};

/// Builds the axum app: `/ws` for the game, static files as fallback.
pub fn build_app(server: SharedServer, client_dir: String) -> axum::Router {
    use axum::routing::get;
    use tower_http::services::ServeDir;

    axum::Router::new().route("/ws", get(ws_handler)).with_state(server).fallback_service(ServeDir::new(client_dir))
}
