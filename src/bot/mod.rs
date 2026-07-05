use serenity::async_trait;
use serenity::client::{Context, EventHandler};
use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::gateway::GatewayIntents;
use serenity::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

mod commands;

// Limites usados para controlar o tamanho da memória de conversa e dos dados dos contratos.
// Eles evitam que o estado em memória cresça sem controle e impacte o desempenho do bot.
const MAX_HISTORY_TURNS: usize = 6;
const MAX_TURN_TEXT_LEN: usize = 700;
const MAX_CONTRACT_ID_LEN: usize = 48;
const MAX_CONTRACT_TITLE_LEN: usize = 80;
const MAX_CONTRACT_TOPIC_LEN: usize = 80;
const MAX_CONTRACT_ATTACHMENT_BYTES: usize = 200_000;
const DISCORD_RESPONSE_CHUNK_LEN: usize = 1900;
const DEFAULT_CONVERSATION_ID: &str = "principal";
const DEFAULT_CONVERSATION_NAME: &str = "Principal";

// Representa um turno simples de conversa entre utilizador e assistente.
// Essa estrutura é persistida para reconstruir o histórico em reinicializações do bot.
#[derive(Clone, Serialize, Deserialize)]
struct ConversationTurn {
    user: String,
    assistant: String,
}

fn extract_key_points(turns: &[ConversationTurn]) -> Vec<String> {
    turns
        .iter()
        .enumerate()
        .filter_map(|(i, turn)| {
            if i % 2 == 0 && !turn.assistant.is_empty() {
                let summary = if turn.assistant.len() > 100 {
                    format!("{}...", &turn.assistant[..100])
                } else {
                    turn.assistant.clone()
                };
                Some(summary)
            } else {
                None
            }
        })
        .collect()
}

#[derive(Clone, Serialize, Deserialize)]
struct StoredConversation {
    name: String,
    turns: Vec<ConversationTurn>,
}

#[derive(Clone, Serialize, Deserialize)]
struct UserConversations {
    active_id: String,
    conversations: HashMap<String, StoredConversation>,
}

#[derive(Serialize, Deserialize)]
struct PersistedConversationEntry {
    channel_id: u64,
    user_id: u64,
    data: UserConversations,
}

#[derive(Serialize, Deserialize)]
struct PersistedContractSessionEntry {
    channel_id: u64,
    user_id: u64,
    data: ContractSession,
}

impl UserConversations {
    fn new() -> Self {
        let mut conversations = HashMap::new();
        conversations.insert(
            DEFAULT_CONVERSATION_ID.to_string(),
            StoredConversation {
                name: DEFAULT_CONVERSATION_NAME.to_string(),
                turns: Vec::new(),
            },
        );

        Self {
            active_id: DEFAULT_CONVERSATION_ID.to_string(),
            conversations,
        }
    }

    fn ensure_active_exists(&mut self) {
        let mut principal_turns = self
            .conversations
            .get(&self.active_id)
            .map(|conversation| conversation.turns.clone())
            .or_else(|| {
                self.conversations
                    .get(DEFAULT_CONVERSATION_ID)
                    .map(|conversation| conversation.turns.clone())
            })
            .unwrap_or_default();

        if principal_turns.len() > MAX_HISTORY_TURNS {
            let extra = principal_turns.len() - MAX_HISTORY_TURNS;
            principal_turns.drain(0..extra);
        }

        self.active_id = DEFAULT_CONVERSATION_ID.to_string();
        self.conversations.clear();
        self.conversations
            .entry(self.active_id.clone())
            .or_insert_with(|| StoredConversation {
                name: DEFAULT_CONVERSATION_NAME.to_string(),
                turns: principal_turns,
            });
    }
}

type ConversationKey = (u64, u64);
type ConversationStore = Arc<Mutex<HashMap<ConversationKey, UserConversations>>>;
type ContractStore = Arc<Mutex<HashMap<ConversationKey, ContractDraft>>>;
type ContractCatalogStore = Arc<Mutex<HashMap<String, StoredContract>>>;
type ContractSessionStore = Arc<Mutex<HashMap<ConversationKey, ContractSession>>>;
type PendingUploadStore = Arc<Mutex<HashMap<ConversationKey, PendingContractUpload>>>;
// message_id -> (channel_id, user_id, contract_id, title, topic, content)
type ContractMessageStore = Arc<Mutex<HashMap<u64, (u64, u64, String, String, String, String)>>>;
// message_id -> (channel_id, user_id) para drafts em criação
type ContractDraftMessageStore = Arc<Mutex<HashMap<u64, (u64, u64)>>>;
// contract_id -> ContractExecutionSummary
type ContractExecutionSummaryStore = Arc<Mutex<HashMap<String, ContractExecutionSummary>>>;

#[derive(Clone, Serialize, Deserialize)]
struct StoredContract {
    id: String,
    title: String,
    topic: String,
    content: String,
    created_at: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
enum SessionState {
    Active,
    Paused,
}

#[derive(Clone, Serialize, Deserialize)]
struct ContractSession {
    contract_id: String,
    contract_title: String,
    contract_topic: String,
    contract_content: String,
    status: SessionState,
    turns: Vec<ConversationTurn>,
    last_updated_at: u64,
}

#[derive(Clone)]
struct PendingContractUpload {
    id: String,
    title: String,
    topic: String,
    content: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct ContractExecutionSummary {
    contract_id: String,
    contract_title: String,
    contract_topic: String,
    total_turns: usize,
    execution_started_at: u64,
    execution_ended_at: u64,
    execution_duration_seconds: u64,
    key_points: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct PersistedExecutionSummaryEntry {
    contract_id: String,
    summary: ContractExecutionSummary,
}

#[derive(Clone, Debug)]
enum ContractStep {
    Title,
    Theme,
    Audience,
    PersonaDefinition,
    ActMethodology,
    Responsibilities,
    Structure,
    Expectations,
}

#[derive(Clone)]
struct ContractDraft {
    step: ContractStep,
    message_id: Option<u64>,
    title: Option<String>,
    theme: Option<String>,
    audience: Option<String>,
    persona: Option<String>,
    act: Option<String>,
    responsibilities: Option<String>,
    structure: Option<String>,
    expectations: Option<String>,
}

impl ContractDraft {
    fn new() -> Self {
        Self {
            step: ContractStep::Title,
            message_id: None,
            title: None,
            theme: None,
            audience: None,
            persona: None,
            act: None,
            responsibilities: None,
            structure: None,
            expectations: None,
        }
    }
}

pub fn status() -> &'static str {
    "bot-ok"
}

// Estado global do bot, incluindo os stores em memória e os caminhos para os arquivos persistidos.
// Esse objeto é compartilhado pelos eventos do Discord e concentra o estado do sistema.
struct Handler {
    guild_id: Option<u64>,
    conversations: ConversationStore,
    conversations_path: String,
    contracts: ContractStore,
    contract_catalog: ContractCatalogStore,
    contract_catalog_path: String,
    contract_sessions: ContractSessionStore,
    contract_sessions_path: String,
    pending_uploads: PendingUploadStore,
    contract_message_store: ContractMessageStore,
    contract_draft_message_store: ContractDraftMessageStore,
    contract_summaries: ContractExecutionSummaryStore,
    contract_summaries_path: String,
    message_content_enabled: bool,
}

#[async_trait]
// Implementa os eventos principais do bot Discord: registro, comandos, mensagens e reações.
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        let register_result = commands::register_commands(&ctx, self.guild_id).await;

        if let Err(err) = register_result {
            eprintln!("Falha ao registrar /status: {err}");
            return;
        }

        if let Some(guild_id) = self.guild_id {
            println!("Comando /status registrado na guild {guild_id}");
        } else {
            println!("Comando /status registrado globalmente");
        }

        println!("Bot ligado como {}", ready.user.name);
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        let Some(command) = commands::as_application_command(interaction) else {
            return;
        };

        commands::dispatch_application_command(self, &ctx, &command).await;
    }

    async fn message(&self, ctx: Context, msg: Message) {
        // Mensagens normais podem alimentar tanto o fluxo de upload de contratos quanto a criação de drafts.
        if process_pending_contract_upload_message(
            &ctx,
            &msg,
            &self.pending_uploads,
        )
        .await
        {
            return;
        }

        process_contract_message(
            &ctx,
            &msg,
            &self.contracts,
            &self.contract_catalog,
            &self.contract_catalog_path,
            &self.contract_message_store,
        ).await;
    }

    async fn reaction_add(&self, ctx: Context, add_reaction: serenity::model::channel::Reaction) {
        let emoji = add_reaction.emoji.to_string();
        let reacting_user = add_reaction.user_id.unwrap_or_default().0;

        // Verificar se é uma reação a um draft em criação
        if emoji == "❌" {
            let draft_store = self.contract_draft_message_store.lock().await;
            if let Some((channel_id, user_id)) = draft_store.get(&add_reaction.message_id.0).cloned() {
                drop(draft_store);

                // Só o criador pode cancelar
                if reacting_user == user_id {
                    let key = (channel_id, user_id);
                    let mut contracts = self.contracts.lock().await;
                    contracts.remove(&key);

                    let ch = serenity::model::prelude::ChannelId(channel_id);
                    let _ = ch.say(&ctx.http, "❌ Criação de contrato cancelada pelo autor.").await;

                    let mut store = self.contract_draft_message_store.lock().await;
                    store.remove(&add_reaction.message_id.0);
                }
                return;
            }
        }

        // Reações funcionam como confirmação para contratos enviados por mensagem: salvar ou cancelar.
        let store = self.contract_message_store.lock().await;
        if let Some((channel_id, author_user_id, _contract_id, title, topic, content)) = store.get(&add_reaction.message_id.0).cloned() {
            drop(store); // Liberar lock antes de fazer chamadas async

            // Guardar: so o autor pode confirmar com 👍
            if emoji == "👍" {
                if reacting_user != author_user_id {
                    return;
                }

                let new_id = generate_next_numeric_contract_id(&self.contract_catalog).await;
                let result = upsert_contract(
                    &self.contract_catalog,
                    &self.contract_catalog_path,
                    &new_id,
                    &title,
                    &topic,
                    &content,
                )
                .await;

                let ch = serenity::model::prelude::ChannelId(channel_id);
                let confirmation = format!(
                    "✅ Contrato guardado com sucesso no catálogo:\n{}\n\n📌 Usa: `/contract_start id:{}` para iniciar uma sessão com este contrato.",
                    result, new_id
                );
                let _ = ch.say(&ctx.http, confirmation).await;

                let mut store = self.contract_message_store.lock().await;
                store.remove(&add_reaction.message_id.0);
                return;
            }

            // Cancelar: so o autor pode cancelar com ❌
            if emoji == "❌" {
                if reacting_user != author_user_id {
                    return;
                }
                let ch = serenity::model::prelude::ChannelId(channel_id);
                let _ = ch.say(&ctx.http, "Operacao cancelada pelo autor.").await;
                let mut store = self.contract_message_store.lock().await;
                store.remove(&add_reaction.message_id.0);
                return;
            }
        }
    }
}

// Gera o próximo identificador numérico para novos contratos guardados no catálogo.
// Isso mantém os IDs estáveis e previsíveis quando um contrato é salvo.
async fn generate_next_numeric_contract_id(contract_catalog: &ContractCatalogStore) -> String {
    let guard = contract_catalog.lock().await;
    let mut max_id: u64 = 0;
    for key in guard.keys() {
        if let Ok(num) = key.parse::<u64>() {
            if num > max_id { max_id = num; }
        }
    }
    (max_id + 1).to_string()
}



// Processa o comando /ask usando a conversa principal e, quando existir, a sessão ativa de contrato.
// O contexto do contrato é incorporado ao prompt para orientar a resposta sem perder a conversa normal.
async fn respond_ask(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
    conversations_path: &str,
    contract_sessions: &ContractSessionStore,
    contract_sessions_path: &str,
) {
    let prompt = get_string_option(command, "prompt").unwrap_or("");

    if prompt.is_empty() {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Prompt vazio. Use /ask prompt:<texto>")
                    })
            })
            .await;
        return;
    }

    let conversation_key = (command.channel_id.0, command.user.id.0);
    let active_contract_session = {
        let guard = contract_sessions.lock().await;
        guard.get(&conversation_key).cloned()
    };
    let contract_session_is_active = matches!(
        active_contract_session.as_ref().map(|session| &session.status),
        Some(SessionState::Active)
    );

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::DeferredChannelMessageWithSource)
                .interaction_response_data(|message| message.ephemeral(contract_session_is_active))
        })
        .await
    {
        eprintln!("Falha ao deferir /ask: {err}");
        return;
    }

    let (active_id, history_snapshot) = {
        let mut guard = conversations.lock().await;
        let user_conversations = guard
            .entry(conversation_key)
            .or_insert_with(UserConversations::new);
        user_conversations.ensure_active_exists();

        let active_id = user_conversations.active_id.clone();
        let active = user_conversations
            .conversations
            .get(&active_id)
            .cloned()
            .unwrap_or(StoredConversation {
                name: DEFAULT_CONVERSATION_NAME.to_string(),
                turns: Vec::new(),
            });

        (active_id, active.turns)
    };

    let structured_history: Vec<(String, String)> = history_snapshot
        .iter()
        .map(|turn| (turn.user.clone(), turn.assistant.clone()))
        .collect();

    let (prompt_to_send, history_to_send, response_header, is_contract_session) =
        if let Some(session) = active_contract_session {
            match session.status {
                SessionState::Active => {
                    let prompt = format!(
                        "Sessao ativa com contrato. Cumpre rigorosamente o contrato abaixo.\nResponde de forma incremental, como continuação da conversa. Mostra apenas o que e novo nesta mensagem e nao repitas a introducao, o contrato, os titulos ou o que ja foi explicado antes.\n\nID: {}\nTitulo: {}\nTopico: {}\n\nContrato:\n{}\n\nPedido do utilizador:\n{}",
                        session.contract_id,
                        session.contract_title,
                        session.contract_topic,
                        session.contract_content,
                        prompt
                    );
                    let session_history = session
                        .turns
                        .iter()
                        .map(|turn| (turn.user.clone(), turn.assistant.clone()))
                        .collect::<Vec<_>>();
                    let header = format!(
                        "[Sessao contrato ativa: {} | {}]",
                        session.contract_id, session.contract_title
                    );
                    (prompt, session_history, header, true)
                }
                SessionState::Paused => (prompt.to_string(), structured_history.clone(), String::new(), false),
            }
        } else {
            (prompt.to_string(), structured_history.clone(), String::new(), false)
        };

    let result_text = match crate::ai::submit_prompt_with_history(&prompt_to_send, &history_to_send).await {
        Ok(answer) => {
            if is_contract_session {
                remember_contract_session_turn(
                    contract_sessions,
                    contract_sessions_path,
                    conversation_key,
                    prompt,
                    &answer,
                )
                .await;
            } else {
                remember_turn(
                    conversations,
                    conversation_key,
                    &active_id,
                    prompt,
                    &answer,
                    conversations_path,
                )
                .await;
            }
            if response_header.is_empty() {
                answer.trim().to_string()
            } else {
                format!("{}\n\n{}", response_header, answer.trim())
            }
        }
        Err(err) => format!("Erro ao consultar IA: {err}"),
    };

    if let Err(err) =
        send_interaction_response_in_chunks(
            ctx,
            command,
            &result_text,
            DISCORD_RESPONSE_CHUNK_LEN,
            is_contract_session,
        )
            .await
    {
        eprintln!("Falha ao responder /ask em partes: {err}");
    }
}

async fn respond_contract_upload(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_catalog: &ContractCatalogStore,
    contract_catalog_path: &str,
    pending_uploads: &PendingUploadStore,
    message_content_enabled: bool,
) {
    let Some(raw_id) = get_string_option(command, "id") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message
                            .content("ID em falta. Usa /contract_upload id:<id> titulo:<texto> topico:<texto> conteudo:<texto>")
                            .ephemeral(true)
                    })
            })
            .await;
        return;
    };

    let Some(raw_title) = get_string_option(command, "titulo") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Titulo em falta. Usa /contract_upload com titulo.").ephemeral(true)
                    })
            })
            .await;
        return;
    };

    let Some(raw_topic) = get_string_option(command, "topico") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Topico em falta. Usa /contract_upload com topico.").ephemeral(true)
                    })
            })
            .await;
        return;
    };

    let contract_id = normalize_contract_id(raw_id);
    if contract_id.is_empty() {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message
                            .content("ID invalido. Usa letras/numeros e separadores simples.")
                            .ephemeral(true)
                    })
            })
            .await;
        return;
    }

    let title: String = raw_title.chars().take(MAX_CONTRACT_TITLE_LEN).collect();
    let topic: String = raw_topic.chars().take(MAX_CONTRACT_TOPIC_LEN).collect();

    if let Some(raw_content) = get_string_option(command, "conteudo") {
        let uploaded_content = raw_content.trim().to_string();
        if uploaded_content.is_empty() {
            let _ = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| {
                            message.content("Conteudo vazio. O contrato precisa de texto.").ephemeral(true)
                        })
                })
                .await;
            return;
        }

        let _content = upsert_contract(
            contract_catalog,
            contract_catalog_path,
            &contract_id,
            &title,
            &topic,
            &uploaded_content,
        )
        .await;

        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| message.ephemeral(true))
            })
            .await;
    } else {
        if !message_content_enabled {
            let _ = command
                .create_interaction_response(&ctx.http, |response| {
                    response
                        .kind(InteractionResponseType::ChannelMessageWithSource)
                        .interaction_response_data(|message| {
                            message
                                .content("Sem conteudo no slash command. Ativa DISCORD_ENABLE_MESSAGE_CONTENT=1 para enviar o contrato na mensagem seguinte, ou passa conteudo diretamente no comando.")
                                .ephemeral(true)
                        })
                })
                .await;
            return;
        }

        let key = (command.channel_id.0, command.user.id.0);
        {
            let mut guard = pending_uploads.lock().await;
            guard.insert(
                key,
                PendingContractUpload {
                    id: contract_id.clone(),
                    title: title.clone(),
                    topic: topic.clone(),
                    content: String::new(),
                },
            );
        }

        let content = format!(
            "Modo upload iniciado para contrato ID '{}' ({}).\nPodes enviar o conteudo em varias mensagens.\nQuando terminares, usa /contract_upload_finish para gravar.\nPara cancelar usa /contract_upload_cancel.",
            contract_id, title
        );

        if let Err(err) = create_interaction_response_in_chunks(
            ctx,
            command,
            &content,
            DISCORD_RESPONSE_CHUNK_LEN,
            true,
        )
            .await
        {
            eprintln!("Falha ao responder /contract_upload: {err}");
        }
    }
}

async fn respond_contract_upload_cancel(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    pending_uploads: &PendingUploadStore,
) {
    let key = (command.channel_id.0, command.user.id.0);
    let content = {
        let mut guard = pending_uploads.lock().await;
        if guard.remove(&key).is_some() {
            "Upload pendente cancelado com sucesso.".to_string()
        } else {
            "Nao existe upload pendente para cancelar neste canal/utilizador.".to_string()
        }
    };

    if let Err(err) =
        create_interaction_response_in_chunks(ctx, command, &content, DISCORD_RESPONSE_CHUNK_LEN, true)
            .await
    {
        eprintln!("Falha ao responder /contract_upload_cancel: {err}");
    }
}

async fn respond_contract_upload_finish(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    pending_uploads: &PendingUploadStore,
    contract_catalog: &ContractCatalogStore,
    contract_catalog_path: &str,
) {
    let key = (command.channel_id.0, command.user.id.0);
    let pending = {
        let mut guard = pending_uploads.lock().await;
        guard.remove(&key)
    };

    let Some(pending) = pending else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message
                            .content("Nao existe upload pendente neste canal/utilizador.")
                            .ephemeral(true)
                    })
            })
            .await;
        return;
    };

    if pending.content.trim().is_empty() {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message
                            .content("O upload pendente nao tem conteudo. Envia texto e depois usa /contract_upload_finish.")
                            .ephemeral(true)
                    })
            })
            .await;
        return;
    }

    let _result = upsert_contract(
        contract_catalog,
        contract_catalog_path,
        &pending.id,
        &pending.title,
        &pending.topic,
        pending.content.trim(),
    )
    .await;

    let _ = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.ephemeral(true))
        })
        .await;
}

async fn process_pending_contract_upload_message(
    ctx: &Context,
    msg: &Message,
    pending_uploads: &PendingUploadStore,
) -> bool {
    if msg.author.bot {
        return false;
    }

    let key = (msg.channel_id.0, msg.author.id.0);
    let has_pending = {
        let guard = pending_uploads.lock().await;
        guard.contains_key(&key)
    };

    if !has_pending {
        return false;
    }

    let content = if !msg.content.trim().is_empty() {
        msg.content.trim().to_string()
    } else {
        match extract_contract_content_from_attachments(msg).await {
            Ok(Some(text)) => text,
            Ok(None) => {
                send_private_pending_upload_feedback(
                    ctx,
                    msg,
                    "Conteudo vazio. Cola o contrato completo numa mensagem normal ou anexa um ficheiro .txt/.md/.markdown/.json. Para cancelar: /contract_upload_cancel.",
                )
                .await;
                return true;
            }
            Err(err) => {
                send_private_pending_upload_feedback(
                    ctx,
                    msg,
                    &format!("Falha ao ler anexo: {err}. Tenta novamente ou usa /contract_upload_cancel."),
                )
                .await;
                return true;
            }
        }
    };

    let buffered_len = {
        let mut guard = pending_uploads.lock().await;
        if let Some(current) = guard.get_mut(&key) {
            if !current.content.is_empty() {
                current.content.push('\n');
            }
            current.content.push_str(&content);
            current.content.len()
        } else {
            0
        }
    };

    let feedback = format!(
        "Parte recebida e adicionada ao upload pendente ({} caracteres acumulados). Quando terminares, usa /contract_upload_finish. Para cancelar: /contract_upload_cancel.",
        buffered_len
    );
    send_private_pending_upload_feedback(ctx, msg, &feedback).await;
    true
}

async fn respond_contract_remove(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_catalog: &ContractCatalogStore,
    contract_catalog_path: &str,
    contract_sessions: &ContractSessionStore,
    contract_sessions_path: &str,
) {
    let Some(raw_contract_id) = get_string_option(command, "id") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("ID em falta. Usa /contract_remove id:<id>").ephemeral(true)
                    })
            })
            .await;
        return;
    };

    let contract_id = normalize_contract_id(raw_contract_id);
    if contract_id.is_empty() {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("ID invalido.").ephemeral(true)
                    })
            })
            .await;
        return;
    }

    let (removed, snapshot) = {
        let mut guard = contract_catalog.lock().await;
        let removed = guard.remove(&contract_id).is_some();
        (removed, guard.clone())
    };

    if !removed {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message
                            .content(format!("Contrato '{}' nao encontrado.", contract_id))
                            .ephemeral(true)
                    })
            })
            .await;
        return;
    }

    if let Err(err) = save_contracts_to_disk(contract_catalog_path, &snapshot) {
        eprintln!("Falha ao persistir remocao de contrato: {err}");
    }

    {
        let mut guard = contract_sessions.lock().await;
        guard.retain(|_, session| session.contract_id != contract_id);
    }

    persist_contract_sessions(contract_sessions, contract_sessions_path).await;

    let content = format!(
        "Contrato '{}' removido com sucesso. Sessoes associadas foram encerradas.",
        contract_id
    );

    if let Err(err) = create_interaction_response_in_chunks(
        ctx,
        command,
        &content,
        DISCORD_RESPONSE_CHUNK_LEN,
        true,
    )
    .await
    {
        eprintln!("Falha ao responder /contract_remove: {err}");
    }
}

async fn send_private_pending_upload_feedback(ctx: &Context, msg: &Message, content: &str) {
    let dm_result = msg
        .author
        .direct_message(&ctx.http, |message| message.content(content))
        .await;

    if dm_result.is_err() {
        let _ = msg.channel_id.say(
            &ctx.http,
            "Nao consegui enviar DM (privado). Ativa DMs do servidor para receber feedback privado.",
        ).await;
    }
}

async fn extract_contract_content_from_attachments(
    msg: &Message,
) -> Result<Option<String>, String> {
    let Some(attachment) = msg.attachments.first() else {
        return Ok(None);
    };

    let filename = attachment.filename.to_ascii_lowercase();
    let allowed = filename.ends_with(".txt")
        || filename.ends_with(".md")
        || filename.ends_with(".markdown")
        || filename.ends_with(".json");

    if !allowed {
        return Err("tipo de ficheiro nao suportado (usa .txt, .md, .markdown ou .json)".to_string());
    }

    let bytes = attachment
        .download()
        .await
        .map_err(|err| format!("erro ao descarregar anexo: {err}"))?;

    if bytes.len() > MAX_CONTRACT_ATTACHMENT_BYTES {
        return Err(format!(
            "ficheiro muito grande (maximo: {} bytes)",
            MAX_CONTRACT_ATTACHMENT_BYTES
        ));
    }

    let text = String::from_utf8(bytes)
        .map_err(|_| "anexo nao esta em UTF-8 valido".to_string())?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    Ok(Some(trimmed.to_string()))
}

async fn upsert_contract(
    contract_catalog: &ContractCatalogStore,
    contract_catalog_path: &str,
    contract_id: &str,
    title: &str,
    topic: &str,
    content: &str,
) -> String {
    let (was_update, snapshot) = {
        let mut guard = contract_catalog.lock().await;
        let was_update = guard.contains_key(contract_id);
        guard.insert(
            contract_id.to_string(),
            StoredContract {
                id: contract_id.to_string(),
                title: title.to_string(),
                topic: topic.to_string(),
                content: content.to_string(),
                created_at: current_unix_timestamp(),
            },
        );
        (was_update, guard.clone())
    };

    if let Err(err) = save_contracts_to_disk(contract_catalog_path, &snapshot) {
        eprintln!("Falha ao persistir catalogo de contratos: {err}");
    }

    if was_update {
        format!(
            "Contrato atualizado com sucesso. ID: {} | Titulo: {} | Topico: {}",
            contract_id, title, topic
        )
    } else {
        format!(
            "Contrato criado com sucesso. ID: {} | Titulo: {} | Topico: {}",
            contract_id, title, topic
        )
    }
}

fn first_sentence(text: &str) -> String {
    // Try to extract a short first sentence or clause to summarise the content.
    let cleaned = text.trim();
    if cleaned.is_empty() {
        return "(sem descrição)".to_string();
    }

    // Split on newlines first, then on sentence terminators.
    let first_line = cleaned
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("");

    let separators = ['.', '!', '?', '|', ';'];
    for sep in &separators {
        if let Some(pos) = first_line.find(*sep) {
            let s = first_line[..pos].trim();
            if !s.is_empty() {
                return s.to_string();
            }
        }
    }

    // Fallback: take up to 80 chars
    let max = 80;
    if first_line.chars().count() > max {
        let short: String = first_line.chars().take(max - 3).collect();
        format!("{}...", short)
    } else if !first_line.is_empty() {
        first_line.to_string()
    } else {
        "(sem descrição)".to_string()
    }
}

fn truncate_topic(topic: &str, max_chars: usize) -> String {
    let t = topic.trim();
    if t.chars().count() <= max_chars {
        return t.to_string();
    }
    let short: String = t.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{}...", short)
}

fn truncate_text(s: &str, max_len: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_len {
        return trimmed.to_string();
    }
    let truncated: String = trimmed.chars().take(max_len).collect();
    match truncated.rfind(' ') {
        Some(idx) => format!("{}...", &truncated[..idx].trim()),
        None => format!("{}...", truncated.trim()),
    }
}

fn learning_goal_from_content(content: &str, max_len: usize) -> String {
    let normalized = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "(objetivo não especificado)".to_string();
    }

    let lowered = normalized.to_ascii_lowercase();
    let cues = [
        "objetivo",
        "objetivos",
        "vai aprender",
        "vai ser capaz",
        "para alunos",
        "para o aluno",
        "aprender",
        "o aluno",
    ];

    for cue in &cues {
        if let Some(pos) = lowered.find(cue) {
            let start = lowered[..pos]
                .rfind(|c: char| c == '.' || c == '!' || c == '?')
                .map(|i| i + 1)
                .unwrap_or(0);
            let end = lowered[pos..]
                .find(|c: char| c == '.' || c == '!' || c == '?')
                .map(|i| pos + i)
                .unwrap_or(normalized.len());

            let snippet = normalized[start..end].trim();
            return truncate_text(snippet, max_len);
        }
    }

    // Fallback to first sentence or truncated content
    let first = first_sentence(content);
    if first == "(sem descrição)" {
        truncate_text(&normalized, max_len)
    } else {
        truncate_text(&first, max_len)
    }
}

fn extract_contract_about_summary(content: &str, topic: &str) -> String {
    let goal = learning_goal_from_content(content, 120);
    let topic_short = truncate_topic(topic, 60);

    if topic.trim().is_empty() {
        format!("Objetivo: {}", goal)
    } else {
        format!("Objetivo: {} — Tópico: {}", goal, topic_short)
    }
}

async fn respond_contract_list(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_catalog: &ContractCatalogStore,
) {
    let content = {
        let guard = contract_catalog.lock().await;
        if guard.is_empty() {
            "Sem contratos registados. Usa /contract_upload para criar o primeiro.".to_string()
        } else {
            let mut contracts: Vec<&StoredContract> = guard.values().collect();
            contracts.sort_by(|a, b| a.title.to_ascii_lowercase().cmp(&b.title.to_ascii_lowercase()));

            let rows = contracts
                .into_iter()
                .map(|contract| {
                    let about_summary = extract_contract_about_summary(&contract.content, &contract.topic);
                    format!(
                        "- ID: {} | Titulo: {} | Sobre: {}",
                        contract.id,
                        contract.title,
                        about_summary
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            format!(
                "Contratos disponiveis:\n{rows}\n\nUsa /contract_start id:<id> para iniciar uma sessao."
            )
        }
    };

    if let Err(err) =
        create_interaction_response_in_chunks(ctx, command, &content, DISCORD_RESPONSE_CHUNK_LEN, true)
            .await
    {
        eprintln!("Falha ao responder /contract_list: {err}");
    }
}

async fn respond_contract_start(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_catalog: &ContractCatalogStore,
    contract_sessions: &ContractSessionStore,
    contract_sessions_path: &str,
) {
    let Some(raw_contract_id) = get_string_option(command, "id") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("ID em falta. Usa /contract_start id:<id>").ephemeral(true)
                    })
            })
            .await;
        return;
    };

    let contract_id = normalize_contract_id(raw_contract_id);
    let contract = {
        let guard = contract_catalog.lock().await;
        guard.get(&contract_id).cloned()
    };

    let Some(contract) = contract else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message
                            .content("Contrato nao encontrado. Usa /contract_list para ver IDs validos.")
                            .ephemeral(true)
                    })
            })
            .await;
        return;
    };

    let key = (command.channel_id.0, command.user.id.0);
    let now = current_unix_timestamp();
    let existing_session = {
        let guard = contract_sessions.lock().await;
        guard.get(&key).cloned()
    };

    if let Some(session) = existing_session {
        match session.status {
            SessionState::Active if session.contract_id == contract.id => {
                let summary_text = summarize_contract_session(&session).await;
                let content = format!(
                    "Ja tens uma sessao ativa para este contrato '{}' (ID: {}).\n\nResumo breve da sessao atual:\n{}\n\nSe quiseres continuar, usa /ask neste canal. Se quiseres guardar o estado para mais tarde, usa /contract_pause.",
                    contract.title, contract.id, summary_text
                );

                if let Err(err) = create_interaction_response_in_chunks(
                    ctx,
                    command,
                    &content,
                    DISCORD_RESPONSE_CHUNK_LEN,
                    true,
                )
                .await
                {
                    eprintln!("Falha ao responder /contract_start: {err}");
                }

                return;
            }
            SessionState::Active => {
                let content = format!(
                    "Ja tens uma sessao ativa para outro contrato: '{}' (ID: {}).\nNao vou criar outra sessão por cima.\nSe queres mudar de contexto, primeiro pausa essa sessão e depois usa /contract_restore id:<id> ou /contract_start id:<id>.",
                    session.contract_title, session.contract_id
                );

                if let Err(err) = create_interaction_response_in_chunks(
                    ctx,
                    command,
                    &content,
                    DISCORD_RESPONSE_CHUNK_LEN,
                    true,
                )
                .await
                {
                    eprintln!("Falha ao responder /contract_start: {err}");
                }

                return;
            }
            SessionState::Paused if session.contract_id == contract.id => {
                let content = {
                    let mut guard = contract_sessions.lock().await;
                    if let Some(current) = guard.get_mut(&key) {
                        current.status = SessionState::Active;
                        current.last_updated_at = now;
                    }

                    format!(
                        "Sessao pausada restaurada para o contrato '{}' (ID: {}).\nA sessão anterior continua guardada e podes retomar com /ask.",
                        contract.title, contract.id
                    )
                };

                persist_contract_sessions(contract_sessions, contract_sessions_path).await;

                if let Err(err) = create_interaction_response_in_chunks(
                    ctx,
                    command,
                    &content,
                    DISCORD_RESPONSE_CHUNK_LEN,
                    true,
                )
                .await
                {
                    eprintln!("Falha ao responder /contract_start: {err}");
                }

                return;
            }
            SessionState::Paused => {
                let content = format!(
                    "Já tens uma sessão pausada para o contrato '{}' (ID: {}).\nPara não perder contexto, usa /contract_restore id:{} para voltar a essa sessão.",
                    session.contract_title, session.contract_id, session.contract_id
                );

                if let Err(err) = create_interaction_response_in_chunks(
                    ctx,
                    command,
                    &content,
                    DISCORD_RESPONSE_CHUNK_LEN,
                    true,
                )
                .await
                {
                    eprintln!("Falha ao responder /contract_start: {err}");
                }

                return;
            }
        }
    }

    {
        let mut guard = contract_sessions.lock().await;
        guard.insert(
            key,
            ContractSession {
                contract_id: contract.id.clone(),
                contract_title: contract.title.clone(),
                contract_topic: contract.topic.clone(),
                contract_content: contract.content.clone(),
                status: SessionState::Active,
                turns: Vec::new(),
                last_updated_at: now,
            },
        );
    }

    persist_contract_sessions(contract_sessions, contract_sessions_path).await;

    let content = format!(
        "Sessao iniciada para o contrato '{}' (ID: {}).\nAs mensagens de /ask deste utilizador+canal passam a seguir este contrato.\nUsa /contract_pause para pausar.",
        contract.title, contract.id
    );

    if let Err(err) =
        create_interaction_response_in_chunks(ctx, command, &content, DISCORD_RESPONSE_CHUNK_LEN, true)
            .await
    {
        eprintln!("Falha ao responder /contract_start: {err}");
    }
}

async fn summarize_contract_session(session: &ContractSession) -> String {
    let status_label = match session.status {
        SessionState::Active => "ativa",
        SessionState::Paused => "pausada",
    };

    if session.turns.is_empty() {
        return format!(
            "Sessao sem interacoes ainda.\nContrato: '{}' (ID: {}, topico: {}).\nEstado: {}.",
            session.contract_title, session.contract_id, session.contract_topic, status_label
        );
    }

    let transcript = session
        .turns
        .iter()
        .enumerate()
        .map(|(idx, turn)| {
            let turn_number = idx + 1;
            format!(
                "Turno {turn_number}\nUtilizador: {}\nAssistente: {}",
                turn.user, turn.assistant
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let prompt = format!(
        "Resume em portugues a sessao de contrato abaixo para retomar mais tarde.\n\
Formato obrigatorio:\n\
1) Estado atual da sessao\n\
2) O que ja foi feito\n\
3) O que falta fazer\n\
4) Proximo passo recomendado\n\n\
Metadados:\nContrato: {} (ID: {}, topico: {})\nEstado: {}\n\nInteracao:\n{}",
        session.contract_title,
        session.contract_id,
        session.contract_topic,
        status_label,
        transcript
    );

    match crate::ai::submit_prompt(&prompt).await {
        Ok(text) => text.trim().to_string(),
        Err(err) => format!("Falha ao gerar resumo com IA: {err}"),
    }
}

async fn respond_contract_pause(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_sessions: &ContractSessionStore,
    contract_sessions_path: &str,
    contract_summaries: &ContractExecutionSummaryStore,
    contract_summaries_path: &str,
) {
    let key = (command.channel_id.0, command.user.id.0);
    let (session_data, content) = {
        let mut guard = contract_sessions.lock().await;
        if let Some(session) = guard.get_mut(&key) {
            match session.status {
                SessionState::Paused => (None, "A sessao ja esta pausada.".to_string()),
                SessionState::Active => {
                    let session_clone = session.clone();
                    session.status = SessionState::Paused;
                    session.last_updated_at = current_unix_timestamp();
                    let msg = format!(
                        "Sessao pausada. Contrato: '{}' (ID: {}). Usa /contract_restore para retomar.",
                        session.contract_title, session.contract_id
                    );
                    (Some(session_clone), msg)
                }
            }
        } else {
            (None, "Nao existe sessao ativa para este utilizador/canal. Usa /contract_start primeiro."
                .to_string())
        }
    };

    // Gerar resumo se houve uma sessão ativa
    if let Some(session) = session_data {
        let summary = ContractExecutionSummary {
            contract_id: session.contract_id.clone(),
            contract_title: session.contract_title.clone(),
            contract_topic: session.contract_topic.clone(),
            total_turns: session.turns.len(),
            execution_started_at: session.last_updated_at - (session.turns.len() as u64 * 60), // Estimativa
            execution_ended_at: current_unix_timestamp(),
            execution_duration_seconds: (session.last_updated_at as u64).saturating_sub(session.last_updated_at as u64 - (session.turns.len() as u64 * 60)),
            key_points: extract_key_points(&session.turns),
        };

        {
            let mut summaries = contract_summaries.lock().await;
            summaries.insert(session.contract_id.clone(), summary);
        }

        persist_execution_summaries(contract_summaries, contract_summaries_path).await;
    }

    persist_contract_sessions(contract_sessions, contract_sessions_path).await;

    if let Err(err) =
        create_interaction_response_in_chunks(ctx, command, &content, DISCORD_RESPONSE_CHUNK_LEN, true)
            .await
    {
        eprintln!("Falha ao responder /contract_pause: {err}");
    }
}

async fn respond_contract_restore(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_sessions: &ContractSessionStore,
    contract_catalog: &ContractCatalogStore,
    contract_sessions_path: &str,
) {
    let key = (command.channel_id.0, command.user.id.0);
    let requested_contract_id = get_string_option(command, "id").map(normalize_contract_id);

    if let Some(contract_id) = requested_contract_id {
        if contract_id.is_empty() {
            let content = "ID invalido. Usa um ID valido em /contract_restore id:<id>.".to_string();
            let _ = create_interaction_response_in_chunks(
                ctx,
                command,
                &content,
                DISCORD_RESPONSE_CHUNK_LEN,
                true,
            )
            .await;
            return;
        }

        let selected_contract = {
            let guard = contract_catalog.lock().await;
            guard.get(&contract_id).cloned()
        };

        let Some(contract) = selected_contract else {
            let content =
                "Contrato nao encontrado para esse ID. Usa /contract_list para ver os IDs.".to_string();
            let _ = create_interaction_response_in_chunks(
                ctx,
                command,
                &content,
                DISCORD_RESPONSE_CHUNK_LEN,
                true,
            )
            .await;
            return;
        };

        let existing_session = {
            let guard = contract_sessions.lock().await;
            guard.get(&key).cloned()
        };

        let content = if let Some(session) = existing_session {
            if session.status == SessionState::Active && session.contract_id == contract.id {
                let summary_text = summarize_contract_session(&session).await;
                format!(
                    "Ja tens esta sessao ativa para '{}' (ID: {}).\n\nResumo breve da sessao atual:\n{}\n\nSe quiseres continuar, usa /ask neste canal. Se quiseres pausar, usa /contract_pause.",
                    contract.title, contract.id, summary_text
                )
            } else if session.status == SessionState::Active {
                format!(
                    "Ja tens uma sessao ativa para o contrato '{}' (ID: {}).\nNao vou restaurar outro contrato por cima.\nSe queres mudar de contexto, primeiro usa /contract_pause nessa sessão.",
                    session.contract_title, session.contract_id
                )
            } else if session.contract_id == contract.id {
                {
                    let mut guard = contract_sessions.lock().await;
                    if let Some(current) = guard.get_mut(&key) {
                        current.status = SessionState::Active;
                        current.last_updated_at = current_unix_timestamp();
                    }

                    format!(
                        "Sessao restaurada para '{}' (ID: {}).\nPodes continuar com /ask neste canal.",
                        contract.title, contract.id
                    )
                }
            } else {
                {
                    let mut guard = contract_sessions.lock().await;
                    if let Some(current) = guard.get_mut(&key) {
                        current.contract_id = contract.id.clone();
                        current.contract_title = contract.title.clone();
                        current.contract_topic = contract.topic.clone();
                        current.contract_content = contract.content.clone();
                        current.status = SessionState::Active;
                        current.turns.clear();
                        current.last_updated_at = current_unix_timestamp();
                    }

                    format!(
                        "Sessao associada e restaurada para contrato '{}' (ID: {}).\nPodes continuar com /ask neste canal.",
                        contract.title, contract.id
                    )
                }
            }
        } else {
            {
                let mut guard = contract_sessions.lock().await;
                guard.insert(
                    key,
                    ContractSession {
                        contract_id: contract.id.clone(),
                        contract_title: contract.title.clone(),
                        contract_topic: contract.topic.clone(),
                        contract_content: contract.content.clone(),
                        status: SessionState::Active,
                        turns: Vec::new(),
                        last_updated_at: current_unix_timestamp(),
                    },
                );
                format!(
                    "Nao havia sessao neste canal/utilizador. Sessao iniciada para '{}' (ID: {}).\nPodes continuar com /ask neste canal.",
                    contract.title, contract.id
                )
            }
        };

        persist_contract_sessions(contract_sessions, contract_sessions_path).await;

        if let Err(err) = create_interaction_response_in_chunks(
            ctx,
            command,
            &content,
            DISCORD_RESPONSE_CHUNK_LEN,
            true,
        )
        .await
        {
            eprintln!("Falha ao responder /contract_restore: {err}");
        }
        return;
    }

    let content = {
        let mut guard = contract_sessions.lock().await;
        if let Some(session) = guard.get_mut(&key) {
            match session.status {
                SessionState::Active => format!(
                    "A sessao ja esta ativa. Contrato: '{}' (ID: {}).\nPodes continuar com /ask neste canal.",
                    session.contract_title, session.contract_id
                ),
                SessionState::Paused => {
                    session.status = SessionState::Active;
                    session.last_updated_at = current_unix_timestamp();
                    format!(
                        "Sessao restaurada. Contrato ativo: '{}' (ID: {}).\nPodes continuar com /ask neste canal.",
                        session.contract_title, session.contract_id
                    )
                }
            }
        } else {
            "Nao existe sessao para restaurar neste utilizador/canal. Usa /contract_start primeiro."
                .to_string()
        }
    };

    persist_contract_sessions(contract_sessions, contract_sessions_path).await;

    if let Err(err) =
        create_interaction_response_in_chunks(ctx, command, &content, DISCORD_RESPONSE_CHUNK_LEN, true)
            .await
    {
        eprintln!("Falha ao responder /contract_restore: {err}");
    }
}

async fn respond_contract_session_summary(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_sessions: &ContractSessionStore,
) {
    let key = (command.channel_id.0, command.user.id.0);
    let session = {
        let guard = contract_sessions.lock().await;
        guard.get(&key).cloned()
    };

    let Some(session) = session else {
        let content = "Nao existe sessao de contrato para resumir neste canal/utilizador.".to_string();
        let _ = create_interaction_response_in_chunks(
            ctx,
            command,
            &content,
            DISCORD_RESPONSE_CHUNK_LEN,
            true,
        )
        .await;
        return;
    };

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::DeferredChannelMessageWithSource)
                .interaction_response_data(|message| message.ephemeral(true))
        })
        .await
    {
        eprintln!("Falha ao deferir /contract_session_summary: {err}");
        return;
    }

    let status_label = match session.status {
        SessionState::Active => "ativa",
        SessionState::Paused => "pausada",
    };

    let summary_text = if session.turns.is_empty() {
        format!(
            "Sessao sem interacoes ainda.\nContrato: '{}' (ID: {}, topico: {}).\nEstado: {}.",
            session.contract_title, session.contract_id, session.contract_topic, status_label
        )
    } else {
        let transcript = session
            .turns
            .iter()
            .enumerate()
            .map(|(idx, turn)| {
                let turn_number = idx + 1;
                format!(
                    "Turno {turn_number}\nUtilizador: {}\nAssistente: {}",
                    turn.user, turn.assistant
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "Resume em portugues a sessao de contrato abaixo para retomar mais tarde.\n\
Formato obrigatorio:\n\
1) Estado atual da sessao\n\
2) O que ja foi feito\n\
3) O que falta fazer\n\
4) Proximo passo recomendado\n\n\
Metadados:\nContrato: {} (ID: {}, topico: {})\nEstado: {}\n\nInteracao:\n{}",
            session.contract_title,
            session.contract_id,
            session.contract_topic,
            status_label,
            transcript
        );

        match crate::ai::submit_prompt(&prompt).await {
            Ok(text) => text.trim().to_string(),
            Err(err) => format!("Falha ao gerar resumo com IA: {err}"),
        }
    };

    let final_content = format!(
        "Resumo da sessao de contrato '{}' (ID: {}):\n\n{}",
        session.contract_title, session.contract_id, summary_text
    );

    if let Err(err) = send_interaction_response_in_chunks(
        ctx,
        command,
        &final_content,
        DISCORD_RESPONSE_CHUNK_LEN,
        true,
    )
    .await
    {
        eprintln!("Falha ao responder /contract_session_summary: {err}");
    }
}

async fn respond_contract_sessions(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_sessions: &ContractSessionStore,
) {
    let user_id = command.user.id.0;
    let current_channel_id = command.channel_id.0;

    let mut sessions = {
        let guard = contract_sessions.lock().await;
        guard
            .iter()
            .filter_map(|((channel_id, session_user_id), session)| {
                if *session_user_id == user_id {
                    Some((*channel_id, session.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };

    if sessions.is_empty() {
        let content = "Nao tens sessoes de contrato abertas/pausadas neste momento.".to_string();
        let _ = create_interaction_response_in_chunks(
            ctx,
            command,
            &content,
            DISCORD_RESPONSE_CHUNK_LEN,
            true,
        )
        .await;
        return;
    }

    sessions.sort_by(|a, b| b.1.last_updated_at.cmp(&a.1.last_updated_at));

    let mut lines = Vec::new();
    lines.push(format!(
        "Sessoes de contrato abertas/pausadas para <@{}> ({}):",
        user_id,
        sessions.len()
    ));

    for (channel_id, session) in sessions {
        let status = match session.status {
            SessionState::Active => "ativa",
            SessionState::Paused => "pausada",
        };
        let current_marker = if channel_id == current_channel_id {
            " [canal atual]"
        } else {
            ""
        };
        lines.push(format!(
            "- Canal <#{}>{}: '{}' (ID: {}, estado: {}, turnos: {}).",
            channel_id,
            current_marker,
            session.contract_title,
            session.contract_id,
            status,
            session.turns.len()
        ));
    }

    lines.push("Para retomar uma sessao: /contract_restore id:<id>.".to_string());
    let content = lines.join("\n");

    if let Err(err) = create_interaction_response_in_chunks(
        ctx,
        command,
        &content,
        DISCORD_RESPONSE_CHUNK_LEN,
        true,
    )
    .await
    {
        eprintln!("Falha ao responder /contract_sessions: {err}");
    }
}

async fn send_interaction_response_in_chunks(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    content: &str,
    chunk_len: usize,
    ephemeral: bool,
) -> serenity::Result<()> {
    let chunks = split_text_for_discord(content, chunk_len);
    let first_chunk = chunks
        .first()
        .cloned()
        .unwrap_or_else(|| "(sem conteudo)".to_string());

    command
        .edit_original_interaction_response(&ctx.http, |response| response.content(first_chunk))
        .await?;

    for chunk in chunks.into_iter().skip(1) {
        command
            .create_followup_message(&ctx.http, |message| {
                message.content(chunk).ephemeral(ephemeral)
            })
            .await?;
    }

    Ok(())
}

async fn create_interaction_response_in_chunks(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    content: &str,
    chunk_len: usize,
    ephemeral: bool,
) -> serenity::Result<()> {
    let chunks = split_text_for_discord(content, chunk_len);
    let first_chunk = chunks
        .first()
        .cloned()
        .unwrap_or_else(|| "(sem conteudo)".to_string());

    command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.content(first_chunk).ephemeral(ephemeral))
        })
        .await?;

    for chunk in chunks.into_iter().skip(1) {
        command
            .create_followup_message(&ctx.http, |message| {
                message.content(chunk).ephemeral(ephemeral)
            })
            .await?;
    }

    Ok(())
}

async fn respond_contract_summary(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_summaries: &ContractExecutionSummaryStore,
) {
    let contract_id = get_string_option(command, "id").unwrap_or("");

    if contract_id.is_empty() {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("ID do contrato nao fornecido. Use /contract_summary id:<contract_id>")
                    })
            })
            .await;
        return;
    }

    let summaries = contract_summaries.lock().await;
    let content = if let Some(summary) = summaries.get(contract_id) {
        format!(
            "📋 **Resumo de Execução - {}** (ID: {})\n\n**Tópico:** {}\n**Turnos:** {}\n**Duração:** {} segundos\n**Início:** <t:{}:f>\n**Fim:** <t:{}:f>\n\n**Pontos-chave:**\n{}",
            summary.contract_title,
            summary.contract_id,
            summary.contract_topic,
            summary.total_turns,
            summary.execution_duration_seconds,
            summary.execution_started_at,
            summary.execution_ended_at,
            summary
                .key_points
                .iter()
                .enumerate()
                .map(|(i, p)| format!("{}. {}", i + 1, p))
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else {
        format!(
            "Nenhum resumo de execução encontrado para o contrato com ID: {}",
            contract_id
        )
    };

    if let Err(err) = create_interaction_response_in_chunks(ctx, command, &content, DISCORD_RESPONSE_CHUNK_LEN, true).await {
        eprintln!("Falha ao responder /contract_summary: {err}");
    }
}



fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

async fn respond_parts(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contract_message_store: &ContractMessageStore,
) {
    let tema = command
        .data
        .options
        .iter()
        .find(|option| option.name == "tema")
        .and_then(|option| option.value.as_ref())
        .and_then(|value| value.as_str())
        .map(str::trim)
        .unwrap_or("");

    if tema.is_empty() {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Tema vazio. Use /parts tema:<assunto>")
                    })
            })
            .await;
        return;
    }

    let contract = build_parts_contract(tema);

    if let Err(err) =
        create_interaction_response_in_chunks(ctx, command, &contract, DISCORD_RESPONSE_CHUNK_LEN, false)
            .await
    {
        eprintln!("Falha ao responder /parts em partes: {err}");
    }

    if let Ok(sent_msg) = command.get_interaction_response(&ctx.http).await {
        let _ = sent_msg
            .react(&ctx.http, serenity::model::prelude::ReactionType::Unicode("👍".to_string()))
            .await;
        let _ = sent_msg
            .react(&ctx.http, serenity::model::prelude::ReactionType::Unicode("❌".to_string()))
            .await;

        let mut store = contract_message_store.lock().await;
        store.insert(
            sent_msg.id.0,
            (
                sent_msg.channel_id.0,
                command.user.id.0,
                String::new(),
                tema.to_string(),
                tema.to_string(),
                contract,
            ),
        );
    }
}

fn build_parts_contract(topic: &str) -> String {
    format!(
        "Semantica PARTS\n\nP - Persona: quem a IA deve ser (perfil/papel).\nA - Act: como deve agir e guiar o raciocinio.\nR - Responsibilities: obrigacoes, limites e qualidade esperada.\nT - Theme: foco central do conteudo.\nS - Structure: formato da interacao e passos da resposta.\n\nContrato PARTS para aprender: {topic}\n\nP - Persona\nTu es um especialista e mentor em {topic}. Ensina de forma clara, progressiva e adaptada ao nivel do estudante.\n\nA - Act\n1. Comeca por diagnosticar o nivel atual com perguntas curtas.\n2. Explica conceitos com exemplos simples e depois aumenta a complexidade.\n3. Usa perguntas de verificacao para confirmar entendimento antes de avancar.\n4. Evita entregar tudo pronto; conduz o estudante a construir a resposta.\n\nR - Responsibilities\n- Justificar cada decisao tecnica/conceitual.\n- Corrigir erros com explicacao do por que.\n- Sinalizar trade-offs e boas praticas.\n- Sugerir mini exercicios e feedback objetivo.\n- Nao inventar factos; quando houver incerteza, indicar explicitamente.\n- Nao terminar a interacao por limite de frases/turnos; continuar ate o estudante confirmar que percebeu.\n\nT - Theme\nAprendizagem de {topic} com foco em compreensao conceitual, pratica guiada e consolidacao.\n\nS - Structure\n1. Entry check: confirmar pre-requisitos.\n2. Exploration: perguntas e intuicao do tema.\n3. Design draft: plano de solucao/resumo mental.\n4. Guided practice: pequenos passos com validacao.\n5. Reflection: o que aprendeu, lacunas e proximo passo.\n6. Conclusao condicionada: so encerrar quando o estudante disser explicitamente que percebeu (ex.: 'percebi', 'entendi').\n\nPrompt pronto para uso:\n\"Usa o contrato PARTS acima e ensina {topic} de forma interativa. Regra obrigatoria: so terminas quando o estudante confirmar que percebeu.\""
    )
}

async fn respond_create_contract(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    contracts: &ContractStore,
    contract_draft_message_store: &ContractDraftMessageStore,
    message_content_enabled: bool,
) {
    if !message_content_enabled {
        let warning = "O fluxo guiado exige MESSAGE_CONTENT intent. Ativa DISCORD_ENABLE_MESSAGE_CONTENT=1 no .env e ativa Message Content Intent no Discord Developer Portal.\n\nAlternativa imediata: usa /parts tema:<assunto>.";
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| message.content(warning))
            })
            .await;
        return;
    }

    let key = (command.channel_id.0, command.user.id.0);

    {
        let mut guard = contracts.lock().await;
        guard.insert(key, ContractDraft::new());
    }

    // Responder brevemente à interação
    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content("Vamos começar! Responde às perguntas abaixo.")
                        .ephemeral(true)
                })
        })
        .await
    {
        eprintln!("Falha ao responder /createcontract: {err}");
        return;
    }

    // Criar a primeira mensagem com pergunta e reação ❌
    let initial_msg_result = command.channel_id.say(&ctx.http,
        "Vamos criar um contrato PARTS completo.\nIsto vai ter 8 perguntas para cobrir todos os pilares.\n\nPergunta 1/8: Qual é o título do contrato? (ex: 'Mitologia Grega para Iniciantes')\n\nReage com ❌ para cancelar a criação."
    ).await;

    if let Ok(initial_msg) = initial_msg_result {
        // Adicionar reação ❌
        let _ = initial_msg.react(&ctx.http, serenity::model::prelude::ReactionType::Unicode("❌".to_string())).await;

        // Guardar a message_id no draft e no store
        {
            let mut guard = contracts.lock().await;
            if let Some(draft) = guard.get_mut(&key) {
                draft.message_id = Some(initial_msg.id.0);
            }
        }

        {
            let mut store = contract_draft_message_store.lock().await;
            store.insert(initial_msg.id.0, (command.channel_id.0, command.user.id.0));
        }
    } else {
        eprintln!("Falha ao criar mensagem inicial para /createcontract");
    }
}

async fn process_contract_message(
    ctx: &Context,
    msg: &Message,
    contracts: &ContractStore,
    _contract_catalog: &ContractCatalogStore,
    _contract_catalog_path: &str,
    contract_message_store: &ContractMessageStore,
) {
    if msg.author.bot {
        return;
    }

    let key = (msg.channel_id.0, msg.author.id.0);
    let input = msg.content.trim();
    if input.is_empty() {
        return;
    }

    let draft = {
        let guard = contracts.lock().await;
        guard.get(&key).cloned()
    };

    let Some(draft) = draft else {
        return;
    };

    match draft.step {
        ContractStep::Title => {
            {
                let mut guard = contracts.lock().await;
                if let Some(current) = guard.get_mut(&key) {
                    current.title = Some(input.to_string());
                    current.step = ContractStep::Theme;
                }
            }
            let _ = msg
                .channel_id
                .say(&ctx.http, "Pergunta 2/8: O que queres aprender? (Tema - T)")
                .await;
        }
        ContractStep::Theme => {
            {
                let mut guard = contracts.lock().await;
                if let Some(current) = guard.get_mut(&key) {
                    current.theme = Some(input.to_string());
                    current.step = ContractStep::Audience;
                }
            }
            let _ = msg
                .channel_id
                .say(&ctx.http, "Pergunta 3/8: Para quem é? Qual é o nível do utilizador? (ex: principiante, intermédio, avançado)")
                .await;
        }
        ContractStep::Audience => {
            {
                let mut guard = contracts.lock().await;
                if let Some(current) = guard.get_mut(&key) {
                    current.audience = Some(input.to_string());
                    current.step = ContractStep::PersonaDefinition;
                }
            }
            let _ = msg
                .channel_id
                .say(&ctx.http, "Pergunta 4/8: Que perfil tem o bot? (especialista, mentor, coach, etc - descreve em poucas palavras)")
                .await;
        }
        ContractStep::PersonaDefinition => {
            let theme = {
                let mut guard = contracts.lock().await;
                if let Some(current) = guard.get_mut(&key) {
                    current.persona = Some(input.to_string());
                    current.step = ContractStep::ActMethodology;
                    current
                        .theme
                        .clone()
                        .unwrap_or_else(|| "tema nao definido".to_string())
                } else {
                    return;
                }
            };
            let validation = validate_persona_with_ai(input).await;
            let _ = msg
                .channel_id
                .say(
                    &ctx.http,
                    format!(
                        "Validacao da persona para '{theme}':\n{}\n\nPergunta 5/8: Como deve agir o bot? Que metodologia usas? (ex: socratica, prática com exemplos, passo-a-passo)",
                        validation
                    ),
                )
                .await;
        }
        ContractStep::ActMethodology => {
            {
                let mut guard = contracts.lock().await;
                if let Some(current) = guard.get_mut(&key) {
                    current.act = Some(input.to_string());
                    current.step = ContractStep::Responsibilities;
                }
            }
            let _ = msg
                .channel_id
                .say(&ctx.http, "Pergunta 6/8: Quais são as responsabilidades do bot? O que deve e não deve fazer? (ex: sempre explicar o porquê, não inventar factos)")
                .await;
        }
        ContractStep::Responsibilities => {
            {
                let mut guard = contracts.lock().await;
                if let Some(current) = guard.get_mut(&key) {
                    current.responsibilities = Some(input.to_string());
                    current.step = ContractStep::Structure;
                }
            }
            let _ = msg
                .channel_id
                .say(&ctx.http, "Pergunta 7/8: Qual é a sequência/estrutura de passos na interação? (ex: check → explore → guide → validate → reflect)")
                .await;
        }
        ContractStep::Structure => {
            {
                let mut guard = contracts.lock().await;
                if let Some(current) = guard.get_mut(&key) {
                    current.structure = Some(input.to_string());
                    current.step = ContractStep::Expectations;
                }
            }
            let _ = msg
                .channel_id
                .say(&ctx.http, "Pergunta 8/8: Quais são as expectativas finais? O que deverá o utilizador conseguir fazer no final?")
                .await;
        }
        ContractStep::Expectations => {
            let (title, theme, audience, persona, act, responsibilities, structure, expectations) = {
                let mut guard = contracts.lock().await;
                if let Some(current) = guard.get_mut(&key) {
                    current.expectations = Some(input.to_string());
                    (
                        current.title.clone().unwrap_or_default(),
                        current.theme.clone().unwrap_or_default(),
                        current.audience.clone().unwrap_or_default(),
                        current.persona.clone().unwrap_or_default(),
                        current.act.clone().unwrap_or_default(),
                        current.responsibilities.clone().unwrap_or_default(),
                        current.structure.clone().unwrap_or_default(),
                        current.expectations.clone().unwrap_or_default(),
                    )
                } else {
                    return;
                }
            };

            let missing_fields: Vec<&str> = vec![
                ("Título", &title),
                ("T - Tema", &theme),
                ("A - Audience (público-alvo)", &audience),
                ("P - Persona", &persona),
                ("A - Act (Ação)", &act),
                ("R - Responsibilities", &responsibilities),
                ("S - Structure", &structure),
                ("Expectations (Expectativas)", &expectations),
            ]
            .into_iter()
            .filter(|(_, value)| value.is_empty())
            .map(|(name, _)| name)
            .collect();

            if !missing_fields.is_empty() {
                let _ = msg.channel_id.say(
                    &ctx.http,
                    format!(
                        "Alguns elementos PARTS faltam:\n{}

Por favor, completa todos os campos antes de guardar o contrato.",
                        missing_fields.iter().map(|f| format!("- {}", f)).collect::<Vec<_>>().join("\n")
                    ),
                ).await;
                return;
            }

            {
                let mut guard = contracts.lock().await;
                guard.remove(&key);
            }

            let contract = build_complete_parts_contract(
                &theme,
                &audience,
                &persona,
                &act,
                &responsibilities,
                &structure,
                &expectations,
            );
            
            let contract_title = title.clone();
            let topic = theme.clone();

            // Publicar o contrato completo numa unica mensagem e deixar apenas duas reacoes finais.
            // Se o texto exceder o limite do Discord, enviamos a versao completa dividida, mas
            // apenas a primeira mensagem recebe o estado de guardado/cancelado.
            let chunks = split_text_for_discord(&contract, DISCORD_RESPONSE_CHUNK_LEN);
            let mut last_sent_message: Option<serenity::model::channel::Message> = None;

            for chunk in chunks.iter() {
                if let Ok(sent_msg) = msg.channel_id.say(&ctx.http, chunk).await {
                    // Guardar a ultima mensagem enviada
                    last_sent_message = Some(sent_msg.clone());
                }
            }

            if let Some(last_msg) = last_sent_message {
                // Adicionar reacoes apenas na ultima mensagem e registar essa mensagem
                let _ = last_msg.react(&ctx.http, serenity::model::prelude::ReactionType::Unicode("👍".to_string())).await;
                let _ = last_msg.react(&ctx.http, serenity::model::prelude::ReactionType::Unicode("❌".to_string())).await;

                let mut store = contract_message_store.lock().await;
                store.insert(
                    last_msg.id.0,
                    (msg.channel_id.0, msg.author.id.0, String::new(), contract_title.clone(), topic.clone(), contract.clone()),
                );
            } else {
                let _ = msg.channel_id.say(&ctx.http, "Nao consegui publicar o contrato.").await;
            }
        }
    }
}

async fn validate_persona_with_ai(persona: &str) -> String {
    let prompt = format!(
        "Avalia se esta persona para um bot educacional esta bem definida: '{persona}'. \
Responde em portugues de forma curta com este formato exato:\n\
- Veredito: (Boa|Parcial|Fraca)\n\
- Justificacao: ...\n\
- Sugestao: ..."
    );

    match crate::ai::submit_prompt(&prompt).await {
        Ok(text) => truncate_for_discord(text.trim(), 700),
        Err(_) => {
            if persona.split_whitespace().count() >= 5 {
                "Veredito: Parcial\nJustificacao: A persona tem alguma especificidade, mas pode ficar mais clara em papel, publico e estilo.\nSugestao: Define especialidade, objetivo pedagogico e tom de comunicacao.".to_string()
            } else {
                "Veredito: Fraca\nJustificacao: A descricao esta curta e generica.\nSugestao: Inclui especialidade, publico-alvo e forma de ensino (ex.: mentor pratico para iniciantes com exemplos).".to_string()
            }
        }
    }
}

fn build_complete_parts_contract(
    theme: &str,
    audience: &str,
    persona: &str,
    act: &str,
    responsibilities: &str,
    structure: &str,
    expectations: &str,
) -> String {
    format!(
        "CONTRATO PARTS COMPLETO\n\n=== CONTEXTO ===\n\nTema: {theme}\nPublico-alvo: {audience}\n\n=== P - PERSONA ===\n\n{persona}\n\n=== A - ACT (Metodologia) ===\n\n{act}\n\n=== R - RESPONSIBILITIES (Responsabilidades) ===\n\n{responsibilities}\n\nRegra obrigatoria de conclusao:\n- A interacao so termina quando o estudante confirmar explicitamente que percebeu.\n- Nao terminar por numero maximo de frases, turnos ou interacoes.\n\n=== T - THEME ===\n\nAprendizagem de: {theme}\nDirigido a: {audience}\n\nFoco: Desenvolver compreensao e capacidade pratica.\n\n=== S - STRUCTURE (Sequencia de Interacao) ===\n\n{structure}\n\nInclui obrigatoriamente uma etapa de verificacao final de compreensao antes de encerrar.\n\n=== EXPECTATIVAS FINAIS ===\n\n{expectations}\n\nReage com 👍 para guardar ou ❌ para cancelar."
    )
}

fn get_string_option<'a>(
    command: &'a ApplicationCommandInteraction,
    name: &str,
) -> Option<&'a str> {
    command
        .data
        .options
        .iter()
        .find(|option| option.name == name)
        .and_then(|option| option.value.as_ref())
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn normalize_contract_id(raw_id: &str) -> String {
    let trimmed = raw_id.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let display: String = trimmed.chars().take(MAX_CONTRACT_ID_LEN).collect();
    let mut id = String::new();
    let mut previous_was_separator = false;
    for character in display.chars() {
        let normalized = character.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() {
            id.push(normalized);
            previous_was_separator = false;
        } else if !previous_was_separator {
            id.push('_');
            previous_was_separator = true;
        }
    }

    id.trim_matches('_').to_string()
}

fn load_conversations_from_disk(storage_path: &str) -> HashMap<ConversationKey, UserConversations> {
    let content = match fs::read_to_string(storage_path) {
        Ok(content) => content,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "Falha ao ler ficheiro de conversas '{}': {err}",
                    storage_path
                );
            }
            return HashMap::new();
        }
    };

    let entries: Vec<PersistedConversationEntry> = match serde_json::from_str(&content) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!(
                "Falha ao parsear ficheiro de conversas '{}': {err}",
                storage_path
            );
            return HashMap::new();
        }
    };

    let mut store = HashMap::new();
    for entry in entries {
        let mut normalized = entry.data;
        normalized.ensure_active_exists();
        store.insert((entry.channel_id, entry.user_id), normalized);
    }

    store
}

fn save_conversations_to_disk(
    storage_path: &str,
    conversations: &HashMap<ConversationKey, UserConversations>,
) -> Result<(), String> {
    let entries: Vec<PersistedConversationEntry> = conversations
        .iter()
        .map(|((channel_id, user_id), data)| PersistedConversationEntry {
            channel_id: *channel_id,
            user_id: *user_id,
            data: data.clone(),
        })
        .collect();

    let json = serde_json::to_string_pretty(&entries)
        .map_err(|err| format!("Erro a serializar conversas: {err}"))?;

    let path = Path::new(storage_path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Erro a criar diretorio de conversas: {err}"))?;
        }
    }

    fs::write(path, json).map_err(|err| format!("Erro a gravar conversas: {err}"))
}

fn load_contract_sessions_from_disk(
    storage_path: &str,
) -> HashMap<ConversationKey, ContractSession> {
    let content = match fs::read_to_string(storage_path) {
        Ok(content) => content,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "Falha ao ler ficheiro de sessoes de contrato '{}': {err}",
                    storage_path
                );
            }
            return HashMap::new();
        }
    };

    let entries: Vec<PersistedContractSessionEntry> = match serde_json::from_str(&content) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!(
                "Falha ao parsear ficheiro de sessoes de contrato '{}': {err}",
                storage_path
            );
            return HashMap::new();
        }
    };

    let mut store = HashMap::new();
    for entry in entries {
        store.insert((entry.channel_id, entry.user_id), entry.data);
    }

    store
}

fn save_contract_sessions_to_disk(
    storage_path: &str,
    contract_sessions: &HashMap<ConversationKey, ContractSession>,
) -> Result<(), String> {
    let entries: Vec<PersistedContractSessionEntry> = contract_sessions
        .iter()
        .map(|((channel_id, user_id), data)| PersistedContractSessionEntry {
            channel_id: *channel_id,
            user_id: *user_id,
            data: data.clone(),
        })
        .collect();

    let json = serde_json::to_string_pretty(&entries)
        .map_err(|err| format!("Erro a serializar sessoes de contrato: {err}"))?;

    let path = Path::new(storage_path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Erro a criar diretorio de sessoes de contrato: {err}"))?;
        }
    }

    fs::write(path, json).map_err(|err| format!("Erro a gravar sessoes de contrato: {err}"))
}

async fn persist_contract_sessions(contract_sessions: &ContractSessionStore, storage_path: &str) {
    let snapshot = {
        let guard = contract_sessions.lock().await;
        guard.clone()
    };

    if let Err(err) = save_contract_sessions_to_disk(storage_path, &snapshot) {
        eprintln!("Falha ao persistir sessoes de contrato: {err}");
    }
}

fn load_contracts_from_disk(storage_path: &str) -> HashMap<String, StoredContract> {
    let content = match fs::read_to_string(storage_path) {
        Ok(content) => content,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "Falha ao ler ficheiro de contratos '{}': {err}",
                    storage_path
                );
            }
            return HashMap::new();
        }
    };

    let entries: Vec<StoredContract> = match serde_json::from_str(&content) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!(
                "Falha ao parsear ficheiro de contratos '{}': {err}",
                storage_path
            );
            return HashMap::new();
        }
    };

    let mut store = HashMap::new();
    for contract in entries {
        store.insert(contract.id.clone(), contract);
    }

    store
}

fn save_contracts_to_disk(
    storage_path: &str,
    contracts: &HashMap<String, StoredContract>,
) -> Result<(), String> {
    let mut entries: Vec<StoredContract> = contracts.values().cloned().collect();
    entries.sort_by(|a, b| a.id.cmp(&b.id));

    let json = serde_json::to_string_pretty(&entries)
        .map_err(|err| format!("Erro a serializar contratos: {err}"))?;

    let path = Path::new(storage_path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Erro a criar diretorio de contratos: {err}"))?;
        }
    }

    fs::write(path, json).map_err(|err| format!("Erro a gravar contratos: {err}"))
}

fn load_execution_summaries_from_disk(storage_path: &str) -> HashMap<String, ContractExecutionSummary> {
    let content = match fs::read_to_string(storage_path) {
        Ok(content) => content,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                eprintln!(
                    "Falha ao ler ficheiro de resumos '{}': {err}",
                    storage_path
                );
            }
            return HashMap::new();
        }
    };

    let entries: Vec<PersistedExecutionSummaryEntry> = match serde_json::from_str(&content) {
        Ok(parsed) => parsed,
        Err(err) => {
            eprintln!(
                "Falha ao parsear ficheiro de resumos '{}': {err}",
                storage_path
            );
            return HashMap::new();
        }
    };

    let mut store = HashMap::new();
    for entry in entries {
        store.insert(entry.contract_id, entry.summary);
    }

    store
}

fn save_execution_summaries_to_disk(
    storage_path: &str,
    summaries: &HashMap<String, ContractExecutionSummary>,
) -> Result<(), String> {
    let mut entries: Vec<PersistedExecutionSummaryEntry> = summaries
        .iter()
        .map(|(contract_id, summary)| PersistedExecutionSummaryEntry {
            contract_id: contract_id.clone(),
            summary: summary.clone(),
        })
        .collect();
    entries.sort_by(|a, b| a.contract_id.cmp(&b.contract_id));

    let json = serde_json::to_string_pretty(&entries)
        .map_err(|err| format!("Erro a serializar resumos: {err}"))?;

    let path = Path::new(storage_path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|err| format!("Erro a criar diretorio de resumos: {err}"))?;
        }
    }

    fs::write(path, json).map_err(|err| format!("Erro a gravar resumos: {err}"))
}

async fn persist_execution_summaries(
    summaries: &ContractExecutionSummaryStore,
    storage_path: &str,
) {
    let guard = summaries.lock().await;
    if let Err(err) = save_execution_summaries_to_disk(storage_path, &*guard) {
        eprintln!("Falha ao persistir resumos: {err}");
    }
}

async fn remember_turn(
    conversations: &ConversationStore,
    key: ConversationKey,
    conversation_id: &str,
    user_prompt: &str,
    assistant_answer: &str,
    conversations_path: &str,
) {
    let user = truncate_for_history(user_prompt, MAX_TURN_TEXT_LEN);
    let assistant = truncate_for_history(assistant_answer, MAX_TURN_TEXT_LEN);
    let snapshot = {
        let mut guard = conversations.lock().await;
        let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
        user_conversations.ensure_active_exists();

        let history = user_conversations
            .conversations
            .entry(conversation_id.to_string())
            .or_insert_with(|| StoredConversation {
                name: conversation_id.replace('_', " "),
                turns: Vec::new(),
            });
        history.turns.push(ConversationTurn { user, assistant });

        if history.turns.len() > MAX_HISTORY_TURNS {
            let extra = history.turns.len() - MAX_HISTORY_TURNS;
            history.turns.drain(0..extra);
        }

        guard.clone()
    };

    if let Err(err) = save_conversations_to_disk(conversations_path, &snapshot) {
        eprintln!("Falha ao persistir conversas em /ask: {err}");
    }
}

async fn remember_contract_session_turn(
    contract_sessions: &ContractSessionStore,
    contract_sessions_path: &str,
    key: ConversationKey,
    user_prompt: &str,
    assistant_answer: &str,
) {
    let user = truncate_for_history(user_prompt, MAX_TURN_TEXT_LEN);
    let assistant = truncate_for_history(assistant_answer, MAX_TURN_TEXT_LEN);

    let mut guard = contract_sessions.lock().await;
    if let Some(session) = guard.get_mut(&key) {
        session.turns.push(ConversationTurn { user, assistant });
        session.last_updated_at = current_unix_timestamp();

        if session.turns.len() > MAX_HISTORY_TURNS {
            let extra = session.turns.len() - MAX_HISTORY_TURNS;
            session.turns.drain(0..extra);
        }
    }

    let snapshot = guard.clone();
    drop(guard);

    if let Err(err) = save_contract_sessions_to_disk(contract_sessions_path, &snapshot) {
        eprintln!("Falha ao persistir turno de sessao de contrato: {err}");
    }
}

async fn respond_conversation_clear(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
    conversations_path: &str,
) {
    let key = (command.channel_id.0, command.user.id.0);

    let snapshot = {
        let mut guard = conversations.lock().await;
        let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
        user_conversations.active_id = DEFAULT_CONVERSATION_ID.to_string();
        user_conversations.conversations.clear();
        user_conversations.conversations.insert(
            DEFAULT_CONVERSATION_ID.to_string(),
            StoredConversation {
                name: DEFAULT_CONVERSATION_NAME.to_string(),
                turns: Vec::new(),
            },
        );
        guard.clone()
    };

    if let Err(err) = save_conversations_to_disk(conversations_path, &snapshot) {
        eprintln!("Falha ao limpar conversa principal: {err}");
    }

    let content = "Conversa principal limpa com sucesso.";
    if let Err(err) = create_interaction_response_in_chunks(
        ctx,
        command,
        content,
        DISCORD_RESPONSE_CHUNK_LEN,
        true,
    )
    .await
    {
        eprintln!("Falha ao responder /conversation_clear: {err}");
    }
}

async fn respond_status(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
) {
    let content = format!("API: {}\nBot: {}", crate::api::status(), status());

    if let Err(err) = create_interaction_response_in_chunks(
        ctx,
        command,
        &content,
        DISCORD_RESPONSE_CHUNK_LEN,
        false,
    )
    .await
    {
        eprintln!("Falha ao responder /status: {err}");
    }
}

fn truncate_for_history(text: &str, max_len: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_len {
        return trimmed.to_string();
    }

    let shortened: String = trimmed.chars().take(max_len.saturating_sub(3)).collect();
    format!("{shortened}...")
}

fn truncate_for_discord(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }

    let truncated: String = text.chars().take(max_len.saturating_sub(3)).collect();
    format!("{truncated}...")
}

fn split_text_for_discord(text: &str, max_len: usize) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return vec!["(sem conteudo)".to_string()];
    }

    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= max_len {
        return vec![trimmed.to_string()];
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut start = 0;

    while start < chars.len() {
        let remaining = chars.len() - start;
        if remaining <= max_len {
            let tail: String = chars[start..].iter().collect();
            let tail = tail.trim();
            if !tail.is_empty() {
                chunks.push(tail.to_string());
            }
            break;
        }

        let end = start + max_len;
        let mut split = end;
        while split > start + 1 && !chars[split - 1].is_whitespace() {
            split -= 1;
        }

        if split == start + 1 {
            split = end;
        }

        let piece: String = chars[start..split].iter().collect();
        let piece = piece.trim();
        if !piece.is_empty() {
            chunks.push(piece.to_string());
        }

        start = split;
        while start < chars.len() && chars[start].is_whitespace() {
            start += 1;
        }
    }

    if chunks.is_empty() {
        vec![trimmed.to_string()]
    } else {
        chunks
    }
}

pub async fn run(token: String, guild_id: Option<u64>) -> serenity::Result<()> {
    let message_content_enabled = env::var("DISCORD_ENABLE_MESSAGE_CONTENT")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "1" || normalized == "true" || normalized == "yes"
        })
        .unwrap_or(false);

    let conversations_path = env::var("CONVERSATIONS_STORE_PATH")
        .unwrap_or_else(|_| "data/conversations.json".to_string());
    let contract_catalog_path = env::var("CONTRACTS_STORE_PATH")
        .unwrap_or_else(|_| "data/contracts.json".to_string());
    let contract_sessions_path = env::var("CONTRACT_SESSIONS_STORE_PATH")
        .unwrap_or_else(|_| "data/contract_sessions.json".to_string());
    let contract_summaries_path = env::var("CONTRACT_SUMMARIES_STORE_PATH")
        .unwrap_or_else(|_| "data/contract_summaries.json".to_string());

    let mut intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILD_MESSAGE_REACTIONS;
    if message_content_enabled {
        intents |= GatewayIntents::MESSAGE_CONTENT;
    }

    println!(
        "Discord MESSAGE_CONTENT intent: {}",
        if message_content_enabled {
            "ativado"
        } else {
            "desativado"
        }
    );

    let loaded_conversations = load_conversations_from_disk(&conversations_path);
    let loaded_contracts = load_contracts_from_disk(&contract_catalog_path);
    let loaded_contract_sessions = load_contract_sessions_from_disk(&contract_sessions_path);
    let loaded_summaries = load_execution_summaries_from_disk(&contract_summaries_path);
    println!(
        "Persistencia de conversas: '{}' ({} registos carregados)",
        conversations_path,
        loaded_conversations.len()
    );
    println!(
        "Catalogo de contratos: '{}' ({} contratos carregados)",
        contract_catalog_path,
        loaded_contracts.len()
    );
    println!(
        "Sessoes de contrato: '{}' ({} registos carregados)",
        contract_sessions_path,
        loaded_contract_sessions.len()
    );
    println!(
        "Resumos de execução: '{}' ({} resumos carregados)",
        contract_summaries_path,
        loaded_summaries.len()
    );

    let conversations = Arc::new(Mutex::new(loaded_conversations));
    let contracts = Arc::new(Mutex::new(HashMap::new()));
    let contract_catalog = Arc::new(Mutex::new(loaded_contracts));
    let contract_sessions = Arc::new(Mutex::new(loaded_contract_sessions));
    let pending_uploads = Arc::new(Mutex::new(HashMap::new()));
    let contract_message_store = Arc::new(Mutex::new(HashMap::new()));
    let contract_draft_message_store = Arc::new(Mutex::new(HashMap::new()));
    let contract_summaries = Arc::new(Mutex::new(loaded_summaries));

    let mut client = Client::builder(token, intents)
        .event_handler(Handler {
            guild_id,
            conversations,
            conversations_path,
            contracts,
            contract_catalog,
            contract_catalog_path,
            contract_sessions,
            contract_sessions_path,
            pending_uploads,
            contract_message_store,
            contract_draft_message_store,
            contract_summaries,
            contract_summaries_path,
            message_content_enabled,
        })
        .await?;

    client.start().await
}
