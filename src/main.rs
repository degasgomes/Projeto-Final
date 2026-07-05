mod api;
mod ai;
mod bot;

use axum::{routing::get, Router};
use std::env;
use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    // Carrega as variáveis de ambiente antes de iniciar os serviços.
    // Isso permite que o bot e a API usem token do Discord, porta HTTP e chaves de IA sem recompilar.
    dotenvy::dotenv().ok();

    let token_opt = env::var("DISCORD_TOKEN").ok();

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

    // O servidor HTTP roda em paralelo para manter o endpoint /status disponível mesmo quando o bot estiver ativo.
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
    // Se existir um token do Discord, o bot é iniciado em segundo plano.
    // Quando o token não está presente, a aplicação continua apenas com a API, o que ajuda em testes locais.
    if let Some(token) = token_opt {
        let bot_task = tokio::spawn(async move {
            bot::run(token, guild_id)
                .await
                .expect("falha ao iniciar bot Discord");
        });

        let _ = tokio::join!(api_task, bot_task);
    } else {
        println!("DISCORD_TOKEN não definido — iniciando apenas o servidor HTTP");
        let _ = api_task.await;
    }
}
