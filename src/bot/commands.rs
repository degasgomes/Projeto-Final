use serenity::client::Context;
use serenity::model::application::command::Command;
use serenity::model::application::command::CommandOptionType;
use serenity::model::application::interaction::application_command::ApplicationCommandInteraction;
use serenity::model::application::interaction::Interaction;
use serenity::model::id::GuildId;

pub async fn register_commands(ctx: &Context, guild_id: Option<u64>) -> serenity::Result<()> {
    if let Some(guild_id) = guild_id {
        GuildId(guild_id)
            .set_application_commands(&ctx.http, |commands| {
                commands
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
                            .name("conversation_clear")
                            .description("Limpa a conversa principal (modo normal)")
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
                    .create_application_command(|command| {
                        command
                            .name("contract_upload")
                            .description("Regista ou atualiza um contrato com metadados")
                            .create_option(|option| {
                                option
                                    .name("id")
                                    .description("Identificador unico do contrato")
                                    .kind(CommandOptionType::String)
                                    .required(true)
                            })
                            .create_option(|option| {
                                option
                                    .name("titulo")
                                    .description("Titulo do contrato")
                                    .kind(CommandOptionType::String)
                                    .required(true)
                            })
                            .create_option(|option| {
                                option
                                    .name("topico")
                                    .description("Topico principal")
                                    .kind(CommandOptionType::String)
                                    .required(true)
                            })
                            .create_option(|option| {
                                option
                                    .name("conteudo")
                                    .description("Conteudo completo do contrato (opcional)")
                                    .kind(CommandOptionType::String)
                                    .required(false)
                            })
                    })
                    .create_application_command(|command| {
                        command
                            .name("contract_upload_cancel")
                            .description("Cancela o modo de upload de contrato pendente")
                    })
                    .create_application_command(|command| {
                        command
                            .name("contract_upload_finish")
                            .description("Finaliza e grava o upload pendente do contrato")
                    })
                    .create_application_command(|command| {
                        command
                            .name("contract_list")
                            .description("Lista os contratos disponiveis")
                    })
                    .create_application_command(|command| {
                        command
                            .name("contract_remove")
                            .description("Remove um contrato do catalogo")
                            .create_option(|option| {
                                option
                                    .name("id")
                                    .description("ID do contrato para remover")
                                    .kind(CommandOptionType::String)
                                    .required(true)
                            })
                    })
                    .create_application_command(|command| {
                        command
                            .name("contract_start")
                            .description("Inicia uma sessao com um contrato")
                            .create_option(|option| {
                                option
                                    .name("id")
                                    .description("ID do contrato para iniciar")
                                    .kind(CommandOptionType::String)
                                    .required(true)
                            })
                    })
                    .create_application_command(|command| {
                        command
                            .name("contract_pause")
                            .description("Pausa a sessao de contrato ativa")
                    })
                    .create_application_command(|command| {
                        command
                            .name("contract_restore")
                            .description("Restaura a sessao de contrato pausada")
                            .create_option(|option| {
                                option
                                    .name("id")
                                    .description("ID do contrato para restaurar/associar (opcional)")
                                    .kind(CommandOptionType::String)
                                    .required(false)
                            })
                    })
                    .create_application_command(|command| {
                        command
                            .name("contract_session_summary")
                            .description("Resume a sessao de contrato atual")
                    })
                    .create_application_command(|command| {
                        command
                            .name("contract_sessions")
                            .description("Lista as sessoes de contrato abertas/pausadas do utilizador")
                    })
            })
            .await
            .map(|_| ())
    } else {
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
        let conversation_clear_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("conversation_clear")
                    .description("Limpa a conversa principal (modo normal)")
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

        let createcontract_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("createcontract")
                    .description("Inicia criacao guiada de um contrato PARTS")
            })
            .await;

        let contract_upload_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_upload")
                    .description("Regista ou atualiza um contrato com metadados")
                    .create_option(|option| {
                        option
                            .name("id")
                            .description("Identificador unico do contrato")
                            .kind(CommandOptionType::String)
                            .required(true)
                    })
                    .create_option(|option| {
                        option
                            .name("titulo")
                            .description("Titulo do contrato")
                            .kind(CommandOptionType::String)
                            .required(true)
                    })
                    .create_option(|option| {
                        option
                            .name("topico")
                            .description("Topico principal")
                            .kind(CommandOptionType::String)
                            .required(true)
                    })
                    .create_option(|option| {
                        option
                            .name("conteudo")
                            .description("Conteudo completo do contrato (opcional)")
                            .kind(CommandOptionType::String)
                            .required(false)
                    })
            })
            .await;

        let contract_upload_cancel_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_upload_cancel")
                    .description("Cancela o modo de upload de contrato pendente")
            })
            .await;

        let contract_upload_finish_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_upload_finish")
                    .description("Finaliza e grava o upload pendente do contrato")
            })
            .await;

        let contract_list_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_list")
                    .description("Lista os contratos disponiveis")
            })
            .await;

        let contract_remove_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_remove")
                    .description("Remove um contrato do catalogo")
                    .create_option(|option| {
                        option
                            .name("id")
                            .description("ID do contrato para remover")
                            .kind(CommandOptionType::String)
                            .required(true)
                    })
            })
            .await;

        let contract_start_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_start")
                    .description("Inicia uma sessao com um contrato")
                    .create_option(|option| {
                        option
                            .name("id")
                            .description("ID do contrato para iniciar")
                            .kind(CommandOptionType::String)
                            .required(true)
                    })
            })
            .await;

        let contract_pause_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_pause")
                    .description("Pausa a sessao de contrato ativa")
            })
            .await;

        let contract_restore_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_restore")
                    .description("Restaura a sessao de contrato pausada")
                    .create_option(|option| {
                        option
                            .name("id")
                            .description("ID do contrato para restaurar/associar (opcional)")
                            .kind(CommandOptionType::String)
                            .required(false)
                    })
            })
            .await;

        let contract_session_summary_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_session_summary")
                    .description("Resume a sessao de contrato atual")
            })
            .await;

        let contract_sessions_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_sessions")
                    .description("Lista as sessoes de contrato abertas/pausadas do utilizador")
            })
            .await;

        let contract_summary_result =
            Command::create_global_application_command(&ctx.http, |command| {
                command
                    .name("contract_summary")
                    .description("Consulta o resumo de execucao de um contrato")
                    .create_option(|option| {
                        option
                            .name("id")
                            .description("ID do contrato para consultar resumo")
                            .kind(CommandOptionType::String)
                            .required(true)
                    })
            })
            .await;

        ask_result
            .and(conversation_clear_result)
            .and(parts_result)
            .and(createcontract_result)
            .and(contract_upload_result)
            .and(contract_upload_cancel_result)
            .and(contract_upload_finish_result)
            .and(contract_list_result)
            .and(contract_remove_result)
            .and(contract_start_result)
            .and(contract_pause_result)
            .and(contract_restore_result)
            .and(contract_session_summary_result)
            .and(contract_sessions_result)
            .and(contract_summary_result)
            .map(|_| ())
    }
}

pub async fn dispatch_application_command(
    handler: &super::Handler,
    ctx: &Context,
    command: &ApplicationCommandInteraction,
) {
    if command.data.name == "ask" {
        super::respond_ask(
            ctx,
            command,
            &handler.conversations,
            &handler.conversations_path,
            &handler.contract_sessions,
            &handler.contract_sessions_path,
        )
        .await;
    } else if command.data.name == "conversation_clear" {
        super::respond_conversation_clear(
            ctx,
            command,
            &handler.conversations,
            &handler.conversations_path,
        )
        .await;
    } else if command.data.name == "parts" {
        super::respond_parts(ctx, command).await;
    } else if command.data.name == "createcontract" {
        super::respond_create_contract(
            ctx,
            command,
            &handler.contracts,
            handler.message_content_enabled,
        )
        .await;
    } else if command.data.name == "contract_upload" {
        super::respond_contract_upload(
            ctx,
            command,
            &handler.contract_catalog,
            &handler.contract_catalog_path,
            &handler.pending_uploads,
            handler.message_content_enabled,
        )
        .await;
    } else if command.data.name == "contract_upload_cancel" {
        super::respond_contract_upload_cancel(ctx, command, &handler.pending_uploads).await;
    } else if command.data.name == "contract_upload_finish" {
        super::respond_contract_upload_finish(
            ctx,
            command,
            &handler.pending_uploads,
            &handler.contract_catalog,
            &handler.contract_catalog_path,
        )
        .await;
    } else if command.data.name == "contract_list" {
        super::respond_contract_list(ctx, command, &handler.contract_catalog).await;
    } else if command.data.name == "contract_remove" {
        super::respond_contract_remove(
            ctx,
            command,
            &handler.contract_catalog,
            &handler.contract_catalog_path,
            &handler.contract_sessions,
            &handler.contract_sessions_path,
        )
        .await;
    } else if command.data.name == "contract_start" {
        super::respond_contract_start(
            ctx,
            command,
            &handler.contract_catalog,
            &handler.contract_sessions,
            &handler.contract_sessions_path,
        )
        .await;
    } else if command.data.name == "contract_pause" {
        super::respond_contract_pause(
            ctx,
            command,
            &handler.contract_sessions,
            &handler.contract_sessions_path,
            &handler.contract_summaries,
            &handler.contract_summaries_path,
        )
        .await;
    } else if command.data.name == "contract_restore" {
        super::respond_contract_restore(
            ctx,
            command,
            &handler.contract_sessions,
            &handler.contract_catalog,
            &handler.contract_sessions_path,
        )
        .await;
    } else if command.data.name == "contract_session_summary" {
        super::respond_contract_session_summary(ctx, command, &handler.contract_sessions).await;
    } else if command.data.name == "contract_sessions" {
        super::respond_contract_sessions(ctx, command, &handler.contract_sessions).await;
    } else if command.data.name == "contract_summary" {
        // Call the shared handler in the bot module
        super::respond_contract_summary(ctx, command, &handler.contract_summaries).await;
    }
}
pub fn as_application_command(
    interaction: Interaction,
) -> Option<ApplicationCommandInteraction> {
    if let Interaction::ApplicationCommand(command) = interaction {
        Some(command)
    } else {
        None
    }
}
