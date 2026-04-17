# ProjetoFinal

Projeto em Rust com:
- Bot Discord
- Backend com OpenAPI
- Integração com IA generativa

## Funcionalidades

- API HTTP de status em `GET /status`
- Comando Discord `/status`
- Comando Discord `/ask prompt:<texto>`
- Comando Discord `/parts tema:<assunto>` para gerar contrato PARTS
- Comando Discord `/createcontract` para criacao guiada de contrato PARTS
- Memoria de contexto multi-turno por utilizador/canal
- Comando `/ask_reset` para limpar contexto
- Conversas multiplas por utilizador/canal (`/ask_new`, `/ask_use`, `/ask_list`)
- Resumo de conversa a qualquer momento (`/ask_summary`)
- Gestao de conversas (`/ask_delete`, `/ask_rename`)
- Exportacao de resumo para Markdown (`/ask_export`)
- Persistencia local de conversas em JSON entre reinicios
- Fallback automatico de Gemini para OpenRouter em erro de quota (`429`)

## Requisitos

- Rust + Cargo
- Token de bot Discord
- Chave Gemini (`GEMINI_API_KEY`)
- Opcional: chave OpenRouter para fallback (`OPENROUTER_API_KEY`)

## Execucao local

Cria um ficheiro `.env` na raiz do projeto com este conteudo:

```env
DISCORD_TOKEN=seu_token_do_bot
DISCORD_GUILD_ID=id_do_servidor_opcional
DISCORD_ENABLE_MESSAGE_CONTENT=0
PORT=3001
GEMINI_API_KEY=sua_chave_gemini
GEMINI_MODEL=gemini-2.0-flash
OPENROUTER_API_KEY=sua_chave_openrouter
OPENROUTER_MODEL=openrouter/auto
CONVERSATIONS_STORE_PATH=data/conversations.json
CONVERSATION_EXPORT_DIR=data/exports
```

Notas:
- `DISCORD_ENABLE_MESSAGE_CONTENT=1` so se ativares tambem o **Message Content Intent** no Discord Developer Portal.
- Se estiver a `0`, o bot arranca sem esse intent e o `/createcontract` mostra instrucoes para ativacao.

Depois executa:

```bash
cargo run
```

Se definir `DISCORD_GUILD_ID`, os comandos sao registrados nessa guild e aparecem quase instantaneamente.
Sem `DISCORD_GUILD_ID`, os comandos sao globais e podem demorar alguns minutos a propagar.

## Comandos Discord

- `/status`: mostra estado da API e bot
- `/hello`: responde "hello world!"
- `/ask prompt:<pergunta>`: envia prompt para IA
- `/parts tema:<assunto>`: explica P-A-R-T-S e devolve um contrato pronto para aprender o tema
- `/createcontract`: inicia criacao **completa** de contrato PARTS com 7 perguntas guiadas (tema, publico, persona, acao, responsabilidades, estrutura, expectativas)
- `/ask_reset`: limpa memoria da conversa
- `/ask_new nome:<texto>`: cria e ativa nova conversa
- `/ask_use nome:<texto>`: ativa conversa existente
- `/ask_list`: lista conversas e indica a ativa
- `/ask_summary [nome:<texto>]`: resume a conversa ativa (ou uma conversa especifica)
- `/ask_delete nome:<texto>`: apaga uma conversa
- `/ask_rename atual:<texto> novo:<texto>`: renomeia conversa
- `/ask_export [nome:<texto>]`: exporta resumo da conversa para ficheiro Markdown

## Como usar /createcontract

O comando `/createcontract` guia-te atraves de 7 perguntas para criar um contrato PARTS completo:

1. **Tema (T)**: O que queres aprender?
2. **Publico** (contexto): Para quem é? Qual é o nível?
3. **Persona (P)**: Que perfil tem o bot? (especialista, mentor, coach)
4. **Acao (A)**: Como deve agir? Que metodologia? (socratica, prática, exemplos)
5. **Responsabilidades (R)**: O que deve e não deve fazer?
6. **Estrutura (S)**: Sequência de passos na interação?
7. **Expectativas**: O que deverá o utilizador conseguir fazer?

O bot valida a persona com IA e gera um contrato PARTS estruturado, pronto para usar diretamente com `/ask`.

## API HTTP

- `GET http://127.0.0.1:3000/status`

Exemplo de resposta da API:

```text
{"api":"api-ok","bot":"bot-ok"}
```

## IA e fallback

Fluxo atual:

1. Tenta Gemini (`GEMINI_API_KEY`).
2. Se Gemini devolver `429`, tenta OpenRouter (`OPENROUTER_API_KEY`).
3. Se ambos falharem, devolve erro descritivo no Discord.

## Producao (sempre online com systemd)

Compilar e copiar binario:

```bash
cargo build --release
sudo mkdir -p /opt/projeto_final
sudo cp target/release/projeto_final /opt/projeto_final/projeto_final
sudo chown -R botdiscord:botdiscord /opt/projeto_final
```

Ficheiro de ambiente:

```bash
sudo nano /etc/projeto_final.env
```

Conteudo exemplo:

```env
DISCORD_TOKEN=seu_token
DISCORD_GUILD_ID=id_opcional
GEMINI_API_KEY=sua_chave_gemini
GEMINI_MODEL=gemini-2.0-flash
OPENROUTER_API_KEY=sua_chave_openrouter
OPENROUTER_MODEL=openrouter/auto
CONVERSATIONS_STORE_PATH=data/conversations.json
CONVERSATION_EXPORT_DIR=data/exports
```

Servico:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now projeto_final
sudo systemctl status projeto_final --no-pager
```

Logs:

```bash
journalctl -u projeto_final -f
```

## Notas de seguranca

- Nao publiques tokens/chaves em screenshots, commits ou chats.
- Se uma chave for exposta, revoga e cria outra.

## Release v1.0.0

- Fluxo PARTS reforcado para nao terminar por limite de interacoes; termina apenas quando o estudante confirma que percebeu.
- Conversas multiplas por utilizador/canal com conversa ativa selecionavel.
- Resumo em qualquer momento da conversa ativa ou por nome.
- Persistencia de conversas em disco (JSON) para manter contexto apos reinicio.
- Exportacao de resumo para Markdown em `data/exports`.
- Novos comandos: `/ask_new`, `/ask_use`, `/ask_list`, `/ask_summary`, `/ask_delete`, `/ask_rename`, `/ask_export`.
