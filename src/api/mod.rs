use axum::Json;
use serde::Serialize;

// Estado simples usado pelo healthcheck da API.
// Serve como sinal rápido de que o serviço está respondendo corretamente.
pub fn status() -> &'static str {
    "api-ok"
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub api: &'static str,
    pub bot: &'static str,
}

// Endpoint de saúde consumido por monitoramento externo ou testes locais.
// Devolve o estado da API e do bot em um único payload para facilitar diagnósticos rápidos.
pub async fn get_status() -> Json<StatusResponse> {
    Json(StatusResponse {
        api: status(),
        bot: crate::bot::status(),
    })
}
