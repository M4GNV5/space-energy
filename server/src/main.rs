use std::sync::{Arc, Mutex};

use server::{build_app, run_game_loop, Server};

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let client_dir = std::env::var("CLIENT_DIR").unwrap_or_else(|_| "../client/dist".into());
    let seed: u64 = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(rand::random);
    println!("world seed: {seed}");

    let server: Arc<Mutex<Server>> = Arc::new(Mutex::new(Server::new(seed)));
    tokio::spawn(run_game_loop(server.clone()));

    let app = build_app(server, client_dir);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    println!("listening on http://localhost:{port}");
    axum::serve(listener, app).await.unwrap();
}
