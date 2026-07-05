mod api;
mod ai;
mod bot;

use axum::{routing::get, Router};
use std::env;
use std::net::SocketAddr;

/// Ponto de entrada da aplicação.
///
/// Inicializa dois serviços concorrentes:
/// - servidor HTTP com o endpoint `/status` (útil para health checks e monitorização);
/// - bot Discord (conecta usando `DISCORD_TOKEN` e regista comandos opcionalmente para
///   a guild definida por `DISCORD_GUILD_ID`).
///
/// Variáveis de ambiente relevantes:
/// - `DISCORD_TOKEN` (obrigatório): token do bot Discord.
/// - `DISCORD_GUILD_ID` (opcional): id da guild para registar comandos localmente.
/// - `PORT` (opcional): porta do servidor HTTP de status (padrão `3000`).
///
/// O estado do bot (conversas, contratos e sessões) é mantido em memória e sincronizado
/// com ficheiros JSON na pasta `data/`. Após alterações nas definições de comandos
/// do Discord, reinicie a aplicação para propagar as mudanças.
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

    // servidor HTTP simples que expõe `/status` para health checks
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

    // iniciar o bot Discord (trata eventos e comandos)
    let bot_task = tokio::spawn(async move {
        bot::run(token, guild_id)
            .await
            .expect("falha ao iniciar bot Discord");
    });

    let _ = tokio::join!(api_task, bot_task);
}
