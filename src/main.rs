mod api;
mod ai;
mod bot;

use axum::{routing::get, Router};
use std::env;
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let token = env::var("DISCORD_TOKEN")
        .expect("defina DISCORD_TOKEN para ligar o bot Discord");

    let http_port = env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(3000);

    let guild_id = env::var("DISCORD_GUILD_ID")
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .expect("DISCORD_GUILD_ID deve ser um número")
        });

    let api_task = tokio::spawn(async move {
        let app = Router::new().route("/status", get(api::get_status));
        let addr = SocketAddr::from(([127, 0, 0, 1], http_port));

        println!("Servidor HTTP em http://{addr}");

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("falha ao abrir porta HTTP configurada");

        axum::serve(listener, app)
            .await
            .expect("falha ao executar servidor HTTP");
    });

    let bot_task = tokio::spawn(async move {
        bot::run(token, guild_id)
            .await
            .expect("falha ao iniciar bot Discord");
    });

    let _ = tokio::join!(api_task, bot_task);
}
