//! End-to-end tests: start the real server on a random port, connect over a
//! websocket, log in, see the starter ship, emit energy, and see rays; check
//! that states stay small and do not pile up for a client that falls behind.
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

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Next message as (json, size on the wire). States are acknowledged, like the real client does.
async fn recv_sized(ws: &mut Ws) -> (Value, usize) {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("timed out waiting for a server message")
            .expect("stream ended")
            .expect("websocket error");
        if let Message::Text(text) = msg {
            let value: Value = serde_json::from_str(&text).expect("valid json");
            if value["t"] == "state" {
                ws.send(Message::text(format!(r#"{{"t":"ack","tick":{}}}"#, value["tick"]))).await.unwrap();
            }
            return (value, text.len());
        }
    }
}

async fn recv_msg(ws: &mut Ws) -> Value {
    recv_sized(ws).await.0
}

/// Connects, logs in and returns the socket and the starter ship's id.
async fn login(port: u16) -> (Ws, u64) {
    let (mut ws, _resp) = tokio_tungstenite::connect_async(format!("ws://127.0.0.1:{port}/ws")).await.expect("connect");
    ws.send(Message::text(r#"{"t":"login","name":"tester"}"#)).await.unwrap();
    let welcome = recv_msg(&mut ws).await;
    assert_eq!(welcome["t"], "welcome");
    assert_eq!(welcome["player"], "tester");
    let ships = welcome["ships"].as_array().expect("ships array");
    assert_eq!(ships.len(), 1, "first login should create exactly one starter ship");
    let ship_id = ships[0].as_u64().unwrap();
    (ws, ship_id)
}

#[tokio::test]
async fn login_emit_and_see_rays() {
    let port = start_test_server().await;
    let (mut ws, ship_id) = login(port).await;

    // The first state is complete: it has our ship with all its blocks as [i, j, material, mass, energy].
    let state = recv_msg(&mut ws).await;
    assert_eq!((&state["t"], &state["reset"]), (&Value::from("state"), &Value::from(true)));
    let ship = state["ships"].as_array().unwrap().iter().find(|s| s["id"].as_u64() == Some(ship_id)).expect("our ship");
    assert_eq!((&ship["new"], &ship["owner"]), (&Value::from(true), &Value::from("tester")));
    assert_eq!(ship["pose"].as_array().unwrap().len(), 6);
    let block = &ship["blocks"][0];
    let (bx, by) = (block[0].as_i64().unwrap(), block[1].as_i64().unwrap());

    let emit = format!(r#"{{"t":"emit","ship":{ship_id},"block":[{bx},{by}],"dir":"all","energy":1000,"mass":0}}"#);
    ws.send(Message::text(emit)).await.unwrap();

    // The next state (or the one after) should carry the beams of this emission.
    let mut saw_beams = false;
    for _ in 0..10 {
        let state = recv_msg(&mut ws).await;
        if state["rays"].as_array().is_some_and(|rays| rays.iter().any(|ray| ray[6] == 1)) {
            saw_beams = true;
            break;
        }
    }
    assert!(saw_beams, "expected to see rays after emitting energy");
}

#[tokio::test]
async fn states_after_the_first_are_small() {
    let port = start_test_server().await;
    let (mut ws, _ship_id) = login(port).await;

    // The default view (1 km around the ship) fills up with the ship and its asteroids.
    let (mut blocks, mut complete_size) = (0, 0);
    for _ in 0..25 {
        let (state, size) = recv_sized(&mut ws).await;
        complete_size += size;
        for ship in state["ships"].as_array().into_iter().flatten().filter(|s| s["new"] == true) {
            blocks += ship["blocks"].as_array().unwrap().len();
        }
    }
    assert!(blocks > 1000, "expected asteroids in view, got {blocks} blocks");

    // From then on, two seconds of play cost far less than sending all of that once.
    let mut size = 0;
    for _ in 0..50 {
        size += recv_sized(&mut ws).await.1;
    }
    assert!(size < complete_size / 4 && size < 60_000, "50 states took {size} bytes, the complete view {complete_size}");
}

#[tokio::test]
async fn a_client_that_falls_behind_gets_fresh_states_not_a_backlog() {
    let port = start_test_server().await;
    let (mut ws, _ship_id) = login(port).await;
    let first_tick = recv_msg(&mut ws).await["tick"].as_u64().unwrap();

    // Two seconds of silence: 50 ticks pass on the server.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Whatever was sent meanwhile is read at once, without acknowledging any of it.
    let mut backlog = Vec::new();
    while let Ok(Some(Ok(msg))) = tokio::time::timeout(Duration::from_millis(20), ws.next()).await {
        if let Message::Text(text) = msg {
            backlog.push(serde_json::from_str::<Value>(&text).unwrap()["tick"].as_u64().unwrap());
        }
    }
    assert!(backlog.len() <= 12, "{} states were queued up", backlog.len());

    // Once the client answers again, it gets the current tick, not the ones it missed.
    ws.send(Message::text(format!(r#"{{"t":"ack","tick":{}}}"#, backlog.last().unwrap()))).await.unwrap();
    let tick = recv_msg(&mut ws).await["tick"].as_u64().unwrap();
    assert!(tick >= first_tick + 45, "got tick {tick}, expected about {}", first_tick + 50);
}
