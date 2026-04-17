use serenity::async_trait;
use serenity::client::{Context, EventHandler};
use serenity::model::application::command::Command;
use serenity::model::application::command::CommandOptionType;
use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::application::interaction::{Interaction, InteractionResponseType};
use serenity::model::channel::Message;
use serenity::model::gateway::Ready;
use serenity::model::gateway::GatewayIntents;
use serenity::model::id::GuildId;
use serenity::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

const MAX_HISTORY_TURNS: usize = 6;
const MAX_TURN_TEXT_LEN: usize = 700;
const MAX_CONVERSATION_NAME_LEN: usize = 40;
const MAX_CONVERSATIONS: usize = 20;
const DEFAULT_CONVERSATION_ID: &str = "principal";
const DEFAULT_CONVERSATION_NAME: &str = "Principal";

#[derive(Clone, Serialize, Deserialize)]
struct ConversationTurn {
    user: String,
    assistant: String,
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
        if self.conversations.contains_key(&self.active_id) {
            return;
        }

        self.active_id = DEFAULT_CONVERSATION_ID.to_string();
        self.conversations
            .entry(self.active_id.clone())
            .or_insert_with(|| StoredConversation {
                name: DEFAULT_CONVERSATION_NAME.to_string(),
                turns: Vec::new(),
            });
    }
}

type ConversationKey = (u64, u64);
type ConversationStore = Arc<Mutex<HashMap<ConversationKey, UserConversations>>>;
type ContractStore = Arc<Mutex<HashMap<ConversationKey, ContractDraft>>>;

#[derive(Clone, Debug)]
enum ContractStep {
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
            step: ContractStep::Theme,
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

struct Handler {
    guild_id: Option<u64>,
    conversations: ConversationStore,
    conversations_path: String,
    contracts: ContractStore,
    message_content_enabled: bool,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        let register_result = if let Some(guild_id) = self.guild_id {
            GuildId(guild_id)
                .set_application_commands(&ctx.http, |commands| {
                    commands
                        .create_application_command(|command| {
                            command
                                .name("status")
                                .description("Mostra estado da API e do bot")
                        })
                        .create_application_command(|command| {
                            command
                                .name("hello")
                                .description("Imprime hello world!")
                        })
                        .create_application_command(|command| {
                            command
                                .name("ask")
                                .description("Envia um prompt para a IA generativa")
                                .create_option(|option| {
                                    option
                                        .name("prompt")
                                        .description("Pergunta para a IA")
                                        .kind(CommandOptionType::String)
                                        .required(true)
                                })
                        })
                        .create_application_command(|command| {
                            command
                                .name("ask_reset")
                                .description("Limpa o contexto da conversa com a IA")
                        })
                        .create_application_command(|command| {
                            command
                                .name("ask_new")
                                .description("Cria e ativa uma nova conversa")
                                .create_option(|option| {
                                    option
                                        .name("nome")
                                        .description("Nome da conversa")
                                        .kind(CommandOptionType::String)
                                        .required(true)
                                })
                        })
                        .create_application_command(|command| {
                            command
                                .name("ask_use")
                                .description("Ativa uma conversa existente")
                                .create_option(|option| {
                                    option
                                        .name("nome")
                                        .description("Nome da conversa para ativar")
                                        .kind(CommandOptionType::String)
                                        .required(true)
                                })
                        })
                        .create_application_command(|command| {
                            command
                                .name("ask_list")
                                .description("Lista as conversas e indica a ativa")
                        })
                        .create_application_command(|command| {
                            command
                                .name("ask_summary")
                                .description("Resume uma conversa a qualquer momento")
                                .create_option(|option| {
                                    option
                                        .name("nome")
                                        .description("Nome da conversa (opcional, usa a ativa por defeito)")
                                        .kind(CommandOptionType::String)
                                        .required(false)
                                })
                        })
                        .create_application_command(|command| {
                            command
                                .name("ask_delete")
                                .description("Apaga uma conversa")
                                .create_option(|option| {
                                    option
                                        .name("nome")
                                        .description("Nome da conversa para apagar")
                                        .kind(CommandOptionType::String)
                                        .required(true)
                                })
                        })
                        .create_application_command(|command| {
                            command
                                .name("ask_rename")
                                .description("Renomeia uma conversa")
                                .create_option(|option| {
                                    option
                                        .name("atual")
                                        .description("Nome atual da conversa")
                                        .kind(CommandOptionType::String)
                                        .required(true)
                                })
                                .create_option(|option| {
                                    option
                                        .name("novo")
                                        .description("Novo nome da conversa")
                                        .kind(CommandOptionType::String)
                                        .required(true)
                                })
                        })
                        .create_application_command(|command| {
                            command
                                .name("ask_export")
                                .description("Exporta resumo da conversa para ficheiro Markdown")
                                .create_option(|option| {
                                    option
                                        .name("nome")
                                        .description("Nome da conversa (opcional, usa a ativa por defeito)")
                                        .kind(CommandOptionType::String)
                                        .required(false)
                                })
                        })
                        .create_application_command(|command| {
                            command
                                .name("parts")
                                .description("Explica PARTS e gera um contrato para um tema")
                                .create_option(|option| {
                                    option
                                        .name("tema")
                                        .description("Tema de aprendizagem")
                                        .kind(CommandOptionType::String)
                                        .required(true)
                                })
                        })
                        .create_application_command(|command| {
                            command
                                .name("createcontract")
                                .description("Inicia criacao guiada de um contrato PARTS")
                        })
                })
                .await
                .map(|_| ())
        } else {
            let status_result = Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("status")
                    .description("Mostra estado da API e do bot")
            })
            .await;

            let hello_result = Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("hello")
                    .description("Imprime hello world!")
            })
            .await;

            let ask_result = Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("ask")
                    .description("Envia um prompt para a IA generativa")
                    .create_option(|option| {
                        option
                            .name("prompt")
                            .description("Pergunta para a IA")
                            .kind(CommandOptionType::String)
                            .required(true)
                    })
            })
            .await;

            let ask_reset_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("ask_reset")
                        .description("Limpa o contexto da conversa com a IA")
                })
                .await;

            let ask_new_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("ask_new")
                        .description("Cria e ativa uma nova conversa")
                        .create_option(|option| {
                            option
                                .name("nome")
                                .description("Nome da conversa")
                                .kind(CommandOptionType::String)
                                .required(true)
                        })
                })
                .await;

            let ask_use_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("ask_use")
                        .description("Ativa uma conversa existente")
                        .create_option(|option| {
                            option
                                .name("nome")
                                .description("Nome da conversa para ativar")
                                .kind(CommandOptionType::String)
                                .required(true)
                        })
                })
                .await;

            let ask_list_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("ask_list")
                        .description("Lista as conversas e indica a ativa")
                })
                .await;

            let ask_summary_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("ask_summary")
                        .description("Resume uma conversa a qualquer momento")
                        .create_option(|option| {
                            option
                                .name("nome")
                                .description("Nome da conversa (opcional, usa a ativa por defeito)")
                                .kind(CommandOptionType::String)
                                .required(false)
                        })
                })
                .await;

            let ask_delete_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("ask_delete")
                        .description("Apaga uma conversa")
                        .create_option(|option| {
                            option
                                .name("nome")
                                .description("Nome da conversa para apagar")
                                .kind(CommandOptionType::String)
                                .required(true)
                        })
                })
                .await;

            let ask_rename_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("ask_rename")
                        .description("Renomeia uma conversa")
                        .create_option(|option| {
                            option
                                .name("atual")
                                .description("Nome atual da conversa")
                                .kind(CommandOptionType::String)
                                .required(true)
                        })
                        .create_option(|option| {
                            option
                                .name("novo")
                                .description("Novo nome da conversa")
                                .kind(CommandOptionType::String)
                                .required(true)
                        })
                })
                .await;

            let ask_export_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("ask_export")
                        .description("Exporta resumo da conversa para ficheiro Markdown")
                        .create_option(|option| {
                            option
                                .name("nome")
                                .description("Nome da conversa (opcional, usa a ativa por defeito)")
                                .kind(CommandOptionType::String)
                                .required(false)
                        })
                })
                .await;

            let parts_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("parts")
                        .description("Explica PARTS e gera um contrato para um tema")
                        .create_option(|option| {
                            option
                                .name("tema")
                                .description("Tema de aprendizagem")
                                .kind(CommandOptionType::String)
                                .required(true)
                        })
                })
                .await;

            let create_contract_result =
                Command::create_global_application_command(&ctx.http, |command| {
                    command
                        .name("createcontract")
                        .description("Inicia criacao guiada de um contrato PARTS")
                })
                .await;

            status_result
                .and(hello_result)
                .and(ask_result)
                .and(ask_reset_result)
                .and(ask_new_result)
                .and(ask_use_result)
                .and(ask_list_result)
                .and(ask_summary_result)
                .and(ask_delete_result)
                .and(ask_rename_result)
                .and(ask_export_result)
                .and(parts_result)
                .and(create_contract_result)
                .map(|_| ())
        };

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
        let Interaction::ApplicationCommand(command) = interaction else {
            return;
        };

        if command.data.name == "status" {
            respond_status(&ctx, &command).await;
        } else if command.data.name == "hello" {
            respond_hello(&ctx, &command).await;
        } else if command.data.name == "ask" {
            respond_ask(
                &ctx,
                &command,
                &self.conversations,
                &self.conversations_path,
            )
            .await;
        } else if command.data.name == "ask_reset" {
            respond_ask_reset(
                &ctx,
                &command,
                &self.conversations,
                &self.conversations_path,
            )
            .await;
        } else if command.data.name == "ask_new" {
            respond_ask_new(
                &ctx,
                &command,
                &self.conversations,
                &self.conversations_path,
            )
            .await;
        } else if command.data.name == "ask_use" {
            respond_ask_use(
                &ctx,
                &command,
                &self.conversations,
                &self.conversations_path,
            )
            .await;
        } else if command.data.name == "ask_list" {
            respond_ask_list(&ctx, &command, &self.conversations).await;
        } else if command.data.name == "ask_summary" {
            respond_ask_summary(&ctx, &command, &self.conversations).await;
        } else if command.data.name == "ask_delete" {
            respond_ask_delete(
                &ctx,
                &command,
                &self.conversations,
                &self.conversations_path,
            )
            .await;
        } else if command.data.name == "ask_rename" {
            respond_ask_rename(
                &ctx,
                &command,
                &self.conversations,
                &self.conversations_path,
            )
            .await;
        } else if command.data.name == "ask_export" {
            respond_ask_export(&ctx, &command, &self.conversations).await;
        } else if command.data.name == "parts" {
            respond_parts(&ctx, &command).await;
        } else if command.data.name == "createcontract" {
            respond_create_contract(
                &ctx,
                &command,
                &self.contracts,
                self.message_content_enabled,
            )
            .await;
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        process_contract_message(&ctx, &msg, &self.contracts).await;
    }
}

async fn respond_status(ctx: &Context, command: &ApplicationCommandInteraction) {
    let content = format!("API: {} | BOT: {}", crate::api::status(), status());

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.content(content.clone()))
        })
        .await
    {
        eprintln!("Falha ao responder /status: {err}");
    }
}

async fn respond_hello(ctx: &Context, command: &ApplicationCommandInteraction) {
    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.content("hello world!"))
        })
        .await
    {
        eprintln!("Falha ao responder /hello: {err}");
    }
}

async fn respond_ask(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
    conversations_path: &str,
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

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response.kind(InteractionResponseType::DeferredChannelMessageWithSource)
        })
        .await
    {
        eprintln!("Falha ao deferir /ask: {err}");
        return;
    }

    let conversation_key = (command.channel_id.0, command.user.id.0);
    let (active_id, active_name, history_snapshot) = {
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

        (active_id, active.name, active.turns)
    };

    let structured_history: Vec<(String, String)> = history_snapshot
        .iter()
        .map(|turn| (turn.user.clone(), turn.assistant.clone()))
        .collect();

    let result_text = match crate::ai::submit_prompt_with_history(prompt, &structured_history).await {
        Ok(answer) => {
            remember_turn(
                conversations,
                conversation_key,
                &active_id,
                prompt,
                &answer,
                conversations_path,
            )
            .await;
            let body = truncate_for_discord(&answer, 1700);
            format!("[Conversa ativa: {active_name}]\n\n{body}")
        }
        Err(err) => format!("Erro ao consultar IA: {err}"),
    };

    if let Err(err) = command
        .edit_original_interaction_response(&ctx.http, |response| response.content(result_text))
        .await
    {
        eprintln!("Falha ao responder /ask: {err}");
    }
}

async fn respond_ask_reset(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
    conversations_path: &str,
) {
    let key = (command.channel_id.0, command.user.id.0);
    let (active_name, snapshot) = {
        let mut guard = conversations.lock().await;
        let active_name = {
            let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
            user_conversations.ensure_active_exists();

            let active_id = user_conversations.active_id.clone();
            if let Some(active) = user_conversations.conversations.get_mut(&active_id) {
                active.turns.clear();
                active.name.clone()
            } else {
                DEFAULT_CONVERSATION_NAME.to_string()
            }
        };

        (active_name, guard.clone())
    };

    if let Err(err) = save_conversations_to_disk(conversations_path, &snapshot) {
        eprintln!("Falha ao persistir conversas em /ask_reset: {err}");
    }

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content(format!(
                        "Contexto limpo para a conversa ativa: {active_name}."
                    ))
                })
        })
        .await
    {
        eprintln!("Falha ao responder /ask_reset: {err}");
    }
}

async fn respond_ask_new(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
    conversations_path: &str,
) {
    let Some(raw_name) = get_string_option(command, "nome") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Nome em falta. Usa /ask_new nome:<texto>")
                    })
            })
            .await;
        return;
    };

    let (conversation_id, display_name) = normalize_conversation_name(raw_name);
    let key = (command.channel_id.0, command.user.id.0);

    let (creation_result, snapshot) = {
        let mut guard = conversations.lock().await;
        let creation_result = {
            let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
            user_conversations.ensure_active_exists();

            if !user_conversations.conversations.contains_key(&conversation_id)
                && user_conversations.conversations.len() >= MAX_CONVERSATIONS
            {
                Err(format!(
                    "Limite de conversas atingido ({MAX_CONVERSATIONS}). Remove algumas para criar novas."
                ))
            } else {
                let already_exists = user_conversations
                    .conversations
                    .contains_key(&conversation_id);
                if !already_exists {
                    user_conversations.conversations.insert(
                        conversation_id.clone(),
                        StoredConversation {
                            name: display_name.clone(),
                            turns: Vec::new(),
                        },
                    );
                }
                user_conversations.active_id = conversation_id;

                if already_exists {
                    Ok(format!(
                        "A conversa '{display_name}' ja existia e foi ativada."
                    ))
                } else {
                    Ok(format!("Conversa '{display_name}' criada e ativada."))
                }
            }
        };

        (creation_result, guard.clone())
    };

    if let Err(err) = save_conversations_to_disk(conversations_path, &snapshot) {
        eprintln!("Falha ao persistir conversas em /ask_new: {err}");
    }

    let content = match creation_result {
        Ok(text) => text,
        Err(err) => err,
    };

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.content(content.clone()))
        })
        .await
    {
        eprintln!("Falha ao responder /ask_new: {err}");
    }
}

async fn respond_ask_use(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
    conversations_path: &str,
) {
    let Some(raw_name) = get_string_option(command, "nome") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Nome em falta. Usa /ask_use nome:<texto>")
                    })
            })
            .await;
        return;
    };

    let (conversation_id, _) = normalize_conversation_name(raw_name);
    let key = (command.channel_id.0, command.user.id.0);

    let (activation_result, snapshot) = {
        let mut guard = conversations.lock().await;
        let activation_result = {
            let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
            user_conversations.ensure_active_exists();

            if let Some(selected) = user_conversations.conversations.get(&conversation_id) {
                let selected_name = selected.name.clone();
                user_conversations.active_id = conversation_id;
                Ok(format!("Conversa ativa alterada para '{selected_name}'."))
            } else {
                let available = format_conversation_list(user_conversations);
                Err(format!(
                    "Conversa nao encontrada. Conversas disponiveis:\n{available}"
                ))
            }
        };

        (activation_result, guard.clone())
    };

    if let Err(err) = save_conversations_to_disk(conversations_path, &snapshot) {
        eprintln!("Falha ao persistir conversas em /ask_use: {err}");
    }

    let content = match activation_result {
        Ok(text) => text,
        Err(err) => err,
    };

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.content(truncate_for_discord(&content, 1900)))
        })
        .await
    {
        eprintln!("Falha ao responder /ask_use: {err}");
    }
}

async fn respond_ask_list(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
) {
    let key = (command.channel_id.0, command.user.id.0);

    let content = {
        let mut guard = conversations.lock().await;
        let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
        user_conversations.ensure_active_exists();

        let available = format_conversation_list(user_conversations);
        format!(
            "Conversas disponiveis:\n{available}\n\nUsa /ask_use para trocar, /ask_summary para resumir, /ask_export para exportar, /ask_rename para renomear e /ask_delete para apagar."
        )
    };

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content(truncate_for_discord(&content, 1900))
                })
        })
        .await
    {
        eprintln!("Falha ao responder /ask_list: {err}");
    }
}

async fn respond_ask_summary(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
) {
    let target_name = get_string_option(command, "nome");
    let key = (command.channel_id.0, command.user.id.0);

    let lookup = {
        let mut guard = conversations.lock().await;
        let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
        user_conversations.ensure_active_exists();

        let conversation_id = target_name
            .map(normalize_conversation_name)
            .map(|(id, _)| id)
            .unwrap_or_else(|| user_conversations.active_id.clone());

        user_conversations
            .conversations
            .get(&conversation_id)
            .cloned()
    };

    let Some(conversation) = lookup else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Conversa nao encontrada. Usa /ask_list para ver as disponiveis.")
                    })
            })
            .await;
        return;
    };

    if conversation.turns.is_empty() {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content(format!(
                            "A conversa '{}' ainda nao tem mensagens para resumir.",
                            conversation.name
                        ))
                    })
            })
            .await;
        return;
    }

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response.kind(InteractionResponseType::DeferredChannelMessageWithSource)
        })
        .await
    {
        eprintln!("Falha ao deferir /ask_summary: {err}");
        return;
    }

    let result_text = match generate_conversation_summary(&conversation).await {
        Ok(summary) => {
            let body = truncate_for_discord(summary.trim(), 1700);
            format!("Resumo da conversa '{}':\n\n{body}", conversation.name)
        }
        Err(err) => format!("Erro ao resumir conversa: {err}"),
    };

    if let Err(err) = command
        .edit_original_interaction_response(&ctx.http, |response| response.content(result_text))
        .await
    {
        eprintln!("Falha ao responder /ask_summary: {err}");
    }
}

async fn respond_ask_delete(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
    conversations_path: &str,
) {
    let Some(raw_name) = get_string_option(command, "nome") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Nome em falta. Usa /ask_delete nome:<texto>")
                    })
            })
            .await;
        return;
    };

    let (conversation_id, _) = normalize_conversation_name(raw_name);
    let key = (command.channel_id.0, command.user.id.0);

    let (deletion_result, snapshot) = {
        let mut guard = conversations.lock().await;
        let deletion_result = {
            let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
            user_conversations.ensure_active_exists();

            if !user_conversations.conversations.contains_key(&conversation_id) {
                let available = format_conversation_list(user_conversations);
                Err(format!(
                    "Conversa nao encontrada. Conversas disponiveis:\n{available}"
                ))
            } else if user_conversations.conversations.len() == 1 {
                if let Some(only) = user_conversations
                    .conversations
                    .get_mut(&user_conversations.active_id)
                {
                    only.turns.clear();
                }
                Ok("Nao e possivel apagar a unica conversa. O historico foi limpo.".to_string())
            } else {
                let removed_name = user_conversations
                    .conversations
                    .remove(&conversation_id)
                    .map(|conversation| conversation.name)
                    .unwrap_or_else(|| "(desconhecida)".to_string());

                if user_conversations.active_id == conversation_id {
                    if user_conversations
                        .conversations
                        .contains_key(DEFAULT_CONVERSATION_ID)
                    {
                        user_conversations.active_id = DEFAULT_CONVERSATION_ID.to_string();
                    } else if let Some(first_id) = user_conversations.conversations.keys().next() {
                        user_conversations.active_id = first_id.clone();
                    }
                }

                let active_name = user_conversations
                    .conversations
                    .get(&user_conversations.active_id)
                    .map(|conversation| conversation.name.clone())
                    .unwrap_or_else(|| DEFAULT_CONVERSATION_NAME.to_string());

                Ok(format!(
                    "Conversa '{removed_name}' apagada. Conversa ativa: '{active_name}'."
                ))
            }
        };

        (deletion_result, guard.clone())
    };

    if let Err(err) = save_conversations_to_disk(conversations_path, &snapshot) {
        eprintln!("Falha ao persistir conversas em /ask_delete: {err}");
    }

    let content = match deletion_result {
        Ok(text) => text,
        Err(err) => err,
    };

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content(truncate_for_discord(&content, 1900))
                })
        })
        .await
    {
        eprintln!("Falha ao responder /ask_delete: {err}");
    }
}

async fn respond_ask_rename(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
    conversations_path: &str,
) {
    let Some(current_name_raw) = get_string_option(command, "atual") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Nome atual em falta. Usa /ask_rename atual:<nome> novo:<nome>")
                    })
            })
            .await;
        return;
    };

    let Some(new_name_raw) = get_string_option(command, "novo") else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Novo nome em falta. Usa /ask_rename atual:<nome> novo:<nome>")
                    })
            })
            .await;
        return;
    };

    let (current_id, _) = normalize_conversation_name(current_name_raw);
    let (new_id, new_display_name) = normalize_conversation_name(new_name_raw);
    let key = (command.channel_id.0, command.user.id.0);

    let (rename_result, snapshot) = {
        let mut guard = conversations.lock().await;
        let rename_result = {
            let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
            user_conversations.ensure_active_exists();

            if !user_conversations.conversations.contains_key(&current_id) {
                let available = format_conversation_list(user_conversations);
                Err(format!(
                    "Conversa nao encontrada. Conversas disponiveis:\n{available}"
                ))
            } else if current_id != new_id
                && user_conversations.conversations.contains_key(&new_id)
            {
                Err("Ja existe uma conversa com esse novo nome.".to_string())
            } else {
                let mut conversation = user_conversations
                    .conversations
                    .remove(&current_id)
                    .unwrap_or(StoredConversation {
                        name: new_display_name.clone(),
                        turns: Vec::new(),
                    });
                conversation.name = new_display_name.clone();
                user_conversations
                    .conversations
                    .insert(new_id.clone(), conversation);

                if user_conversations.active_id == current_id {
                    user_conversations.active_id = new_id;
                }

                Ok(format!("Conversa renomeada para '{new_display_name}'."))
            }
        };

        (rename_result, guard.clone())
    };

    if let Err(err) = save_conversations_to_disk(conversations_path, &snapshot) {
        eprintln!("Falha ao persistir conversas em /ask_rename: {err}");
    }

    let content = match rename_result {
        Ok(text) => text,
        Err(err) => err,
    };

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content(truncate_for_discord(&content, 1900))
                })
        })
        .await
    {
        eprintln!("Falha ao responder /ask_rename: {err}");
    }
}

async fn respond_ask_export(
    ctx: &Context,
    command: &ApplicationCommandInteraction,
    conversations: &ConversationStore,
) {
    let target_name = get_string_option(command, "nome");
    let key = (command.channel_id.0, command.user.id.0);

    let lookup = {
        let mut guard = conversations.lock().await;
        let user_conversations = guard.entry(key).or_insert_with(UserConversations::new);
        user_conversations.ensure_active_exists();

        let conversation_id = target_name
            .map(normalize_conversation_name)
            .map(|(id, _)| id)
            .unwrap_or_else(|| user_conversations.active_id.clone());

        user_conversations
            .conversations
            .get(&conversation_id)
            .cloned()
    };

    let Some(conversation) = lookup else {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content("Conversa nao encontrada. Usa /ask_list para ver as disponiveis.")
                    })
            })
            .await;
        return;
    };

    if conversation.turns.is_empty() {
        let _ = command
            .create_interaction_response(&ctx.http, |response| {
                response
                    .kind(InteractionResponseType::ChannelMessageWithSource)
                    .interaction_response_data(|message| {
                        message.content(format!(
                            "A conversa '{}' ainda nao tem mensagens para exportar.",
                            conversation.name
                        ))
                    })
            })
            .await;
        return;
    }

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response.kind(InteractionResponseType::DeferredChannelMessageWithSource)
        })
        .await
    {
        eprintln!("Falha ao deferir /ask_export: {err}");
        return;
    }

    let result_text = match generate_conversation_summary(&conversation).await {
        Ok(summary) => {
            let export_dir = env::var("CONVERSATION_EXPORT_DIR")
                .unwrap_or_else(|_| "data/exports".to_string());
            let timestamp = current_unix_timestamp();
            let filename = format!(
                "{}_{}.md",
                sanitize_for_filename(&conversation.name),
                timestamp
            );
            let path = Path::new(&export_dir).join(filename);

            let write_result = fs::create_dir_all(&export_dir)
                .map_err(|err| format!("Erro a criar diretorio de exportacao: {err}"))
                .and_then(|_| {
                    let markdown = format!(
                        "# Resumo da Conversa\n\nNome: {}\nGerado em: {}\n\n## Resumo\n\n{}\n",
                        conversation.name,
                        timestamp,
                        summary.trim()
                    );
                    fs::write(&path, markdown)
                        .map_err(|err| format!("Erro a gravar ficheiro de exportacao: {err}"))
                });

            match write_result {
                Ok(()) => format!(
                    "Resumo exportado com sucesso para: {}",
                    path.display()
                ),
                Err(err) => format!("Falha ao exportar resumo: {err}"),
            }
        }
        Err(err) => format!("Erro ao gerar resumo para exportacao: {err}"),
    };

    if let Err(err) = command
        .edit_original_interaction_response(&ctx.http, |response| response.content(result_text))
        .await
    {
        eprintln!("Falha ao responder /ask_export: {err}");
    }
}

fn current_unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn sanitize_for_filename(raw: &str) -> String {
    let mut value = String::new();
    let mut previous_was_separator = false;
    for character in raw.chars() {
        let normalized = character.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() {
            value.push(normalized);
            previous_was_separator = false;
        } else if !previous_was_separator {
            value.push('_');
            previous_was_separator = true;
        }
    }

    let value = value.trim_matches('_').to_string();
    if value.is_empty() {
        "conversa".to_string()
    } else {
        value
    }
}

async fn generate_conversation_summary(conversation: &StoredConversation) -> Result<String, crate::ai::AiError> {
    let transcript = conversation
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
        "Resume em portugues a seguinte interacao entre estudante e assistente. \
Seja objetivo e util para retomar a conversa mais tarde.\n\
Formato obrigatorio:\n\
1) Resumo curto\n\
2) Pontos principais aprendidos\n\
3) Duvidas em aberto\n\
4) Proximo passo recomendado\n\n\
Interacao:\n{transcript}"
    );

    crate::ai::submit_prompt(&prompt).await
}

async fn respond_parts(ctx: &Context, command: &ApplicationCommandInteraction) {
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
    let response_text = truncate_for_discord(&contract, 1900);

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| message.content(response_text.clone()))
        })
        .await
    {
        eprintln!("Falha ao responder /parts: {err}");
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

    if let Err(err) = command
        .create_interaction_response(&ctx.http, |response| {
            response
                .kind(InteractionResponseType::ChannelMessageWithSource)
                .interaction_response_data(|message| {
                    message.content(
                        "Vamos criar um contrato PARTS completo.\nIsto vai ter 7 perguntas para cobrir todos os pilares.\n\nPergunta 1/7: O que queres aprender? (Tema - T)",
                    )
                })
        })
        .await
    {
        eprintln!("Falha ao responder /createcontract: {err}");
    }
}

async fn process_contract_message(ctx: &Context, msg: &Message, contracts: &ContractStore) {
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
                .say(&ctx.http, "Pergunta 2/7: Para quem é? Qual é o nível do utilizador? (ex: principiante, intermédio, avançado)")
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
                .say(&ctx.http, "Pergunta 3/7: Que perfil tem o bot? (especialista, mentor, coach, etc - descreve em poucas palavras)")
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
                        "Validacao da persona para '{theme}':\n{}\n\nPergunta 4/7: Como deve agir o bot? Que metodologia usas? (ex: socratica, prática com exemplos, passo-a-passo)",
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
                .say(&ctx.http, "Pergunta 5/7: Quais são as responsabilidades do bot? O que deve e não deve fazer? (ex: sempre explicar o porquê, não inventar factos)")
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
                .say(&ctx.http, "Pergunta 6/7: Qual é a sequência/estrutura de passos na interação? (ex: check → explore → guide → validate → reflect)")
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
                .say(&ctx.http, "Pergunta 7/7: Quais são as expectativas finais? O que deverá o utilizador conseguir fazer no final?")
                .await;
        }
        ContractStep::Expectations => {
            let (theme, audience, persona, act, responsibilities, structure, expectations) = {
                let mut guard = contracts.lock().await;
                if let Some(current) = guard.get_mut(&key) {
                    current.expectations = Some(input.to_string());
                    (
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
            
            let parts: Vec<&str> = contract.split("---SPLIT---").collect();
            for part in parts {
                let response_text = truncate_for_discord(part.trim(), 1900);
                if !response_text.is_empty() {
                    let _ = msg.channel_id.say(&ctx.http, response_text).await;
                }
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
        "CONTRATO PARTS COMPLETO\n\n=== CONTEXTO ===\n\nTema: {theme}\nPublico-alvo: {audience}\n\n---SPLIT---\n\n=== P - PERSONA ===\n\n{persona}\n\n---SPLIT---\n\n=== A - ACT (Metodologia) ===\n\n{act}\n\n---SPLIT---\n\n=== R - RESPONSIBILITIES (Responsabilidades) ===\n\n{responsibilities}\n\nRegra obrigatoria de conclusao:\n- A interacao so termina quando o estudante confirmar explicitamente que percebeu.\n- Nao terminar por numero maximo de frases, turnos ou interacoes.\n\n---SPLIT---\n\n=== T - THEME ===\n\nAprendizagem de: {theme}\nDirigido a: {audience}\n\nFoco: Desenvolver compreensao e capacidade pratica.\n\n---SPLIT---\n\n=== S - STRUCTURE (Sequencia de Interacao) ===\n\n{structure}\n\nInclui obrigatoriamente uma etapa de verificacao final de compreensao antes de encerrar.\n\n---SPLIT---\n\n=== EXPECTATIVAS FINAIS ===\n\n{expectations}\n\n---SPLIT---\n\n=== PROMPT PRONTO PARA USAR ===\n\nUsa este contrato PARTS de forma rigorosa para guiar a aprendizagem de {theme} para alguem ao nivel de {audience}.\n\nRegra critica: so podes concluir a sessao quando o estudante disser claramente que percebeu; nao concluas por limite de interacoes/frases.\n\nP (Persona): {persona}\nA (Acao): {act}\nR (Responsabilidades): {responsibilities}\nT (Tema): {theme}\nS (Estrutura): {structure}"
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

fn normalize_conversation_name(raw_name: &str) -> (String, String) {
    let trimmed = raw_name.trim();
    let display_name: String = if trimmed.is_empty() {
        DEFAULT_CONVERSATION_NAME.to_string()
    } else {
        trimmed.chars().take(MAX_CONVERSATION_NAME_LEN).collect()
    };

    let mut id = String::new();
    let mut previous_was_separator = false;
    for character in display_name.chars() {
        let normalized = character.to_ascii_lowercase();
        if normalized.is_ascii_alphanumeric() {
            id.push(normalized);
            previous_was_separator = false;
        } else if !previous_was_separator {
            id.push('_');
            previous_was_separator = true;
        }
    }

    let id = id.trim_matches('_').to_string();
    let safe_id = if id.is_empty() {
        DEFAULT_CONVERSATION_ID.to_string()
    } else {
        id
    };

    (safe_id, display_name)
}

fn format_conversation_list(user_conversations: &UserConversations) -> String {
    let mut items: Vec<(String, usize, bool)> = user_conversations
        .conversations
        .iter()
        .map(|(id, conversation)| {
            (
                conversation.name.clone(),
                conversation.turns.len(),
                id == &user_conversations.active_id,
            )
        })
        .collect();

    items.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));

    if items.is_empty() {
        return "- (sem conversas)".to_string();
    }

    items
        .into_iter()
        .map(|(name, turns, active)| {
            if active {
                format!("- {name} [ativa] ({turns} turnos)")
            } else {
                format!("- {name} ({turns} turnos)")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        store.insert((entry.channel_id, entry.user_id), entry.data);
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

    let mut intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_MESSAGES;
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
    println!(
        "Persistencia de conversas: '{}' ({} registos carregados)",
        conversations_path,
        loaded_conversations.len()
    );

    let conversations = Arc::new(Mutex::new(loaded_conversations));
    let contracts = Arc::new(Mutex::new(HashMap::new()));

    let mut client = Client::builder(token, intents)
        .event_handler(Handler {
            guild_id,
            conversations,
            conversations_path,
            contracts,
            message_content_enabled,
        })
        .await?;

    client.start().await
}
