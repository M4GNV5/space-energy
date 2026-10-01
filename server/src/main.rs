use std::sync::{Arc, Mutex};

use server::{build_app, run_game_loop, Server};

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let client_dir = std::env::var("CLIENT_DIR").unwrap_or_else(|_| "../client/dist".into());
    let save_file = std::path::PathBuf::from(std::env::var("SAVE_FILE").unwrap_or_else(|_| "save.json".into()));

    let loaded = server::save::load(&save_file).unwrap_or_else(|e| panic!("cannot read {}: {e}", save_file.display()));
    let server = loaded.unwrap_or_else(|| {
        let seed: u64 = std::env::var("SEED").ok().and_then(|s| s.parse().ok()).unwrap_or_else(rand::random);
        Server::new(seed)
    });
    println!("world seed: {}, player ships: {}", server.world.seed, server.world.ships.len());

    let server: Arc<Mutex<Server>> = Arc::new(Mutex::new(server));
    tokio::spawn(run_game_loop(server.clone()));
    tokio::spawn(server::save::run_save_loop(server.clone(), save_file.clone()));

    // Ctrl-C saves once more before the process exits.
    let shutdown_server = server.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        match server::save::save(&shutdown_server, &save_file) {
            Ok(()) => println!("saved to {}", save_file.display()),
            Err(e) => eprintln!("save failed: {e}"),
        }
        std::process::exit(0);
    });

    let app = build_app(server, client_dir);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap();
    println!("listening on http://localhost:{port}");
    axum::serve(listener, app).await.unwrap();
}
