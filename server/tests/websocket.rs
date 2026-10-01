//! End-to-end test: start the real server on a random port, connect over a
//! websocket, log in, see the starter ship, emit energy, and see rays.
//!
//! `ServerMsg` only derives `Serialize` (the server never needs to parse its
//! own outgoing messages), so responses are checked as raw `serde_json::Value`.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use server::{build_app, run_game_loop, Server};
use tokio_tungstenite::tungstenite::Message;

async fn start_test_server() -> u16 {
    let srv = Arc::new(Mutex::new(Server::new(12345)));
    tokio::spawn(run_game_loop(srv.clone()));
    let app = build_app(srv, "does-not-exist".into());
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    port
}

async fn recv_msg<S>(ws: &mut S) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for a server message")
            .expect("stream ended")
            .expect("websocket error");
        if let Message::Text(text) = msg {
            return serde_json::from_str(&text).expect("valid json");
        }
    }
}

#[tokio::test]
async fn login_emit_and_see_rays() {
    let port = start_test_server().await;
    let url = format!("ws://127.0.0.1:{port}/ws");
    let (mut ws, _resp) = tokio_tungstenite::connect_async(url).await.expect("connect");

    ws.send(Message::text(r#"{"t":"login","name":"tester"}"#)).await.unwrap();

    let welcome = recv_msg(&mut ws).await;
    assert_eq!(welcome["t"], "welcome");
    assert_eq!(welcome["player"], "tester");
    let ships = welcome["ships"].as_array().expect("ships array");
    assert_eq!(ships.len(), 1, "first login should create exactly one starter ship");
    let ship_id = ships[0].as_u64().unwrap();

    // Widen the view so the state includes our ship regardless of default centring.
    ws.send(Message::text(r#"{"t":"view","x":0,"y":0,"r":3000}"#)).await.unwrap();

    // Find a state message containing our ship with a block to emit from.
    let mut block: Option<(i64, i64)> = None;
    for _ in 0..50 {
        let state = recv_msg(&mut ws).await;
        if state["t"] != "state" {
            continue;
        }
        if let Some(ships) = state["ships"].as_array() {
            if let Some(ship) = ships.iter().find(|s| s["id"].as_u64() == Some(ship_id)) {
                if let Some(b) = ship["blocks"].as_array().and_then(|bs| bs.first()) {
                    let p = b["p"].as_array().unwrap();
                    block = Some((p[0].as_i64().unwrap(), p[1].as_i64().unwrap()));
                    break;
                }
            }
        }
    }
    let (bx, by) = block.expect("should eventually see our ship's blocks in a state message");

    let emit = format!(r#"{{"t":"emit","ship":{ship_id},"block":[{bx},{by}],"dir":"all","energy":1000,"mass":0}}"#);
    ws.send(Message::text(emit)).await.unwrap();

    // The next state (or the one after) should carry rays from this emission.
    let mut saw_rays = false;
    for _ in 0..10 {
        let state = recv_msg(&mut ws).await;
        if state["t"] == "state" {
            if let Some(rays) = state["rays"].as_array() {
                if !rays.is_empty() {
                    saw_rays = true;
                    break;
                }
            }
        }
    }
    assert!(saw_rays, "expected to see rays after emitting energy");
}
