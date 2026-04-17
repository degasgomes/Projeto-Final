use axum::Json;
use serde::Serialize;

pub fn status() -> &'static str {
    "api-ok"
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub api: &'static str,
    pub bot: &'static str,
}

pub async fn get_status() -> Json<StatusResponse> {
    Json(StatusResponse {
        api: status(),
        bot: crate::bot::status(),
    })
}
