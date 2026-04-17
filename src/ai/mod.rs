use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::env;
use std::error::Error;
use std::fmt;
use std::time::Duration;
use tokio::time::sleep;

const GEMINI_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta";
const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const MAX_RETRIES: usize = 2;

#[derive(Debug)]
pub enum AiError {
    MissingApiKey,
    MissingOpenRouterApiKey,
    EmptyPrompt,
    Http(reqwest::Error),
    ApiError { status: StatusCode, body: String },
    InvalidResponse,
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingApiKey => write!(f, "defina GEMINI_API_KEY para usar a IA"),
            Self::MissingOpenRouterApiKey => {
                write!(f, "defina OPENROUTER_API_KEY para usar fallback de IA")
            }
            Self::EmptyPrompt => write!(f, "o prompt nao pode estar vazio"),
            Self::Http(err) => write!(f, "falha de rede ao chamar provedor de IA: {err}"),
            Self::ApiError { status, body } => {
                if *status == StatusCode::TOO_MANY_REQUESTS {
                    write!(
                        f,
                        "Provedor de IA devolveu HTTP 429 (limite/quota). Aguarda 20-60s e tenta novamente. Detalhes: {}",
                        compact_error_body(body)
                    )
                } else {
                    write!(
                        f,
                        "Provedor de IA devolveu erro HTTP {status}. Detalhes: {}",
                        compact_error_body(body)
                    )
                }
            }
            Self::InvalidResponse => write!(f, "resposta do provedor sem texto utilizavel"),
        }
    }
}

impl Error for AiError {}

impl From<reqwest::Error> for AiError {
    fn from(value: reqwest::Error) -> Self {
        Self::Http(value)
    }
}

fn compact_error_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() > 220 {
        let short: String = compact.chars().take(217).collect();
        format!("{short}...")
    } else {
        compact
    }
}

#[derive(Serialize)]
struct GenerateContentRequest {
    contents: Vec<Content>,
}

#[derive(Serialize, Deserialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Deserialize)]
struct GenerateContentResponse {
    candidates: Option<Vec<Candidate>>,
}

#[derive(Deserialize)]
struct Candidate {
    content: Option<Content>,
}

#[derive(Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<OpenRouterMessage>,
}

#[derive(Serialize, Deserialize)]
struct OpenRouterMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenRouterResponse {
    choices: Vec<OpenRouterChoice>,
}

#[derive(Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterMessage,
}

pub async fn submit_prompt(prompt: &str) -> Result<String, AiError> {
    let trimmed_prompt = prompt.trim();
    if trimmed_prompt.is_empty() {
        return Err(AiError::EmptyPrompt);
    }

    match submit_with_gemini(trimmed_prompt).await {
        Ok(text) => Ok(text),
        Err(AiError::ApiError {
            status: StatusCode::TOO_MANY_REQUESTS,
            body,
        }) => {
            if env::var("OPENROUTER_API_KEY").is_ok() {
                submit_with_openrouter(trimmed_prompt).await
            } else {
                Err(AiError::ApiError {
                    status: StatusCode::TOO_MANY_REQUESTS,
                    body: format!(
                        "{body} | Define OPENROUTER_API_KEY para fallback automatico quando Gemini excede quota"
                    ),
                })
            }
        }
        Err(err) => Err(err),
    }
}

pub async fn submit_prompt_with_history(
    prompt: &str,
    history: &[(String, String)],
) -> Result<String, AiError> {
    let mut enriched_prompt = String::from(
        "Usa o contexto da conversa anterior para responder de forma consistente. \
Se o contexto nao for relevante, responde apenas ao pedido atual.\n\
Regra para interacoes de ensino (incluindo contrato PARTS): nao concluas a sessao por limite de frases, \
turnos ou interacoes; so conclui quando o estudante confirmar explicitamente que percebeu.\n\n",
    );

    for (index, (user, assistant)) in history.iter().enumerate() {
        let turn = index + 1;
        enriched_prompt.push_str(&format!(
            "Turno {turn}\nUtilizador: {user}\nAssistente: {assistant}\n\n"
        ));
    }

    enriched_prompt.push_str(&format!("Pedido atual\nUtilizador: {prompt}"));
    submit_prompt(&enriched_prompt).await
}

async fn submit_with_gemini(prompt: &str) -> Result<String, AiError> {
    let api_key = env::var("GEMINI_API_KEY").map_err(|_| AiError::MissingApiKey)?;
    let model = env::var("GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.0-flash".to_string());

    let endpoint = format!("{GEMINI_BASE_URL}/models/{model}:generateContent");
    let request_body = GenerateContentRequest {
        contents: vec![Content {
            parts: vec![Part {
                text: prompt.to_string(),
            }],
        }],
    };

    let client = reqwest::Client::new();
    let mut attempt = 0;
    loop {
        let response = client
            .post(&endpoint)
            .query(&[("key", &api_key)])
            .json(&request_body)
            .send()
            .await?;

        if response.status().is_success() {
            let parsed: GenerateContentResponse = response.json().await?;
            let generated_text = parsed
                .candidates
                .and_then(|mut candidates| candidates.drain(..).next())
                .and_then(|candidate| candidate.content)
                .and_then(|content| content.parts.into_iter().next())
                .map(|part| part.text)
                .ok_or(AiError::InvalidResponse)?;

            return Ok(generated_text);
        }

        let status = response.status();
        if status == StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RETRIES {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(2_u64.pow((attempt as u32) + 1));

            sleep(Duration::from_secs(retry_after)).await;
            attempt += 1;
            continue;
        }

        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "erro ao ler corpo".to_string());
        return Err(AiError::ApiError { status, body });
    }
}

async fn submit_with_openrouter(prompt: &str) -> Result<String, AiError> {
    let api_key = env::var("OPENROUTER_API_KEY").map_err(|_| AiError::MissingOpenRouterApiKey)?;
    let model = env::var("OPENROUTER_MODEL")
        .unwrap_or_else(|_| "openrouter/auto".to_string());

    let request_body = OpenRouterRequest {
        model,
        messages: vec![OpenRouterMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
        }],
    };

    let client = reqwest::Client::new();
    let response = client
        .post(OPENROUTER_URL)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response
            .text()
            .await
            .unwrap_or_else(|_| "erro ao ler corpo".to_string());
        return Err(AiError::ApiError { status, body });
    }

    let parsed: OpenRouterResponse = response.json().await?;
    parsed
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message.content)
        .filter(|content| !content.trim().is_empty())
        .ok_or(AiError::InvalidResponse)
}
