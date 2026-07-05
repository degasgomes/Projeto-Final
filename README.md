# ProjetoFinal

Projeto em Rust com bot Discord, API HTTP e integração com IA generativa para conversar, criar contratos PARTS e acompanhar sessões de aprendizagem.

## Funcionalidades

- API HTTP com endpoint de estado em `GET /status`
- Bot Discord com comandos slash para:
  - conversa normal com IA (`/ask`)
  - limpeza da conversa principal (`/conversation_clear`)
  - geração rápida de contratos PARTS (`/parts`)
  - criação guiada de contratos (`/createcontract`)
  - catálogo e gestão de contratos (`/contract_upload`, `/contract_list`, `/contract_remove`)
  - sessões de estudo por contrato (`/contract_start`, `/contract_pause`, `/contract_restore`)
  - resumo de execução e sessões (`/contract_summary`, `/contract_session_summary`, `/contract_sessions`)
  - verificação do estado do bot (`/status`)
- Persistência local em JSON entre reinícios para:
  - conversas normais
  - catálogo de contratos
  - resumos de execução de contratos
  - sessões de contrato
- Fallback automático de Gemini para OpenRouter em caso de erro `429`

## Requisitos

- Rust e Cargo
- Token do bot Discord
- Chave Gemini (`GEMINI_API_KEY`)
- Chave OpenRouter opcional (`OPENROUTER_API_KEY`)
Projeto em Rust com bot Discord, API HTTP e integracao com IA generativa.

## Funcionalidades

- API HTTP de status em `GET /status`
- Comando `/ask` com memoria curta de contexto
- Conversa normal unica por utilizador/canal: `principal`
- Comando `/conversation_clear` para limpar a conversa principal
- Comando `/parts` para gerar contrato PARTS rapido por tema
- Comando `/createcontract` com criacao guiada (7 perguntas)
- Catalogo de contratos (`/contract_upload`, `/contract_list`, `/contract_remove`)
- Sessao de aprendizagem por contrato (`/contract_start`, `/contract_pause`, `/contract_restore`)
- Resumo de execucao por contrato (`/contract_summary`)
- Resumo e listagem de sessoes (`/contract_session_summary`, `/contract_sessions`)
- Persistencia local em JSON entre reinicios:
	- conversas normais
	- catalogo de contratos
	- resumos de execucao de contratos
	- sessoes de contrato
- Fallback automatico de Gemini para OpenRouter em erro de quota (`429`)

## Requisitos

- Rust + Cargo
- Token do bot Discord
- Chave Gemini (`GEMINI_API_KEY`)
- Opcional: chave OpenRouter (`OPENROUTER_API_KEY`)

## Configuração local

Crie um ficheiro `.env` na pasta do projeto com conteúdo semelhante a:
Cria um ficheiro `.env` na raiz do projeto:

```env
DISCORD_TOKEN=seu_token_do_bot
DISCORD_GUILD_ID=id_do_servidor_opcional
DISCORD_ENABLE_MESSAGE_CONTENT=0
PORT=3000
PORT=3001

GEMINI_API_KEY=sua_chave_gemini
GEMINI_MODEL=gemini-2.0-flash

OPENROUTER_API_KEY=sua_chave_openrouter
OPENROUTER_MODEL=openrouter/auto
```

Notas:
- `DISCORD_ENABLE_MESSAGE_CONTENT=1` exige ativar também o Message Content Intent no Discord Developer Portal.
- Com `DISCORD_ENABLE_MESSAGE_CONTENT=0`, alguns fluxos guiados por mensagens normais ficam limitados.
- Se `DISCORD_TOKEN` não estiver definido, a aplicação inicia apenas o servidor HTTP.

## Execução

Na pasta do projeto, execute:

CONVERSATIONS_STORE_PATH=data/conversations.json
CONTRACTS_STORE_PATH=data/contracts.json
CONTRACT_SUMMARIES_STORE_PATH=data/contract_summaries.json
CONTRACT_SESSIONS_STORE_PATH=data/contract_sessions.json
```

Notas:
- `DISCORD_ENABLE_MESSAGE_CONTENT=1` exige ativar tambem o Message Content Intent no Discord Developer Portal.
- Se estiver a `0`, fluxos guiados por mensagem normal (ex.: `/createcontract` e upload pendente sem conteudo no slash) ficam limitados.

Executa:

```bash
cargo run
```

Se definir `DISCORD_GUILD_ID`, os comandos aparecem quase instantaneamente na guild.
Sem esse valor, os comandos são registados globalmente e podem demorar alguns minutos a aparecer.
Se definires `DISCORD_GUILD_ID`, os comandos aparecem quase instantaneamente na guild.
Sem `DISCORD_GUILD_ID`, os comandos sao globais e podem demorar alguns minutos.

## Comandos Discord

### Conversa normal

- `/ask prompt:<pergunta>`: envia uma pergunta para a IA usando a conversa principal
- `/conversation_clear`: limpa o histórico da conversa principal

### Contratos PARTS

- `/parts tema:<assunto>`: gera um contrato PARTS rápido para um tema
- `/createcontract`: inicia a criação guiada com várias perguntas
- `/contract_upload id:<id> titulo:<titulo> topico:<topico> [conteudo:<texto>]`: regista ou atualiza um contrato
- `/contract_upload_finish`: finaliza um upload pendente por mensagens
- `/contract_upload_cancel`: cancela um upload pendente
- `/contract_list`: lista os contratos registados
- `/contract_remove id:<id>`: remove um contrato

### Sessões de contrato

- `/contract_start id:<id>`: inicia uma sessão com um contrato
- `/contract_pause`: pausa a sessão ativa
- `/contract_restore [id:<id>]`: retoma uma sessão pausada ou associa um contrato específico
- `/contract_summary id:<contract_id>`: mostra o resumo de execução de um contrato
- `/contract_session_summary`: resume a sessão atual
- `/contract_sessions`: lista as sessões abertas ou pausadas do utilizador
- `/status`: mostra o estado do bot e da API

## Fluxo de upload de contrato

Quando se usa `/contract_upload` sem `conteudo`, o bot entra em modo de upload pendente para esse utilizador/canal:

1. Envia uma ou várias mensagens com o conteúdo do contrato.
2. Usa `/contract_upload_finish` para gravar.
3. Usa `/contract_upload_cancel` para abortar.

## Persistência de dados

Os ficheiros de dados são guardados localmente em:

- `data/conversations.json`
- `data/contracts.json`
- `data/contract_summaries.json`
- `data/contract_sessions.json`

## API HTTP

Endpoint disponível em:

- `GET http://127.0.0.1:3000/status`

Exemplo de resposta:

```json
{"api":"api-ok","bot":"bot-ok"}
```

- `/ask prompt:<pergunta>`: pergunta para a IA usando a conversa principal
- `/conversation_clear`: limpa o historico da conversa principal

### Contratos PARTS

- `/parts tema:<assunto>`: gera contrato PARTS rapido
- `/createcontract`: criacao guiada com 7 perguntas
- `/contract_upload id:<id> titulo:<titulo> topico:<topico> [conteudo:<texto>]`: cria/atualiza contrato
- `/contract_upload_finish`: finaliza upload pendente por mensagens
- `/contract_upload_cancel`: cancela upload pendente
- `/contract_list`: lista contratos registados
- `/contract_remove id:<id>`: remove contrato

### Sessoes de contrato

- `/contract_start id:<id>`: inicia sessao com um contrato
- `/contract_pause`: pausa a sessao ativa
- `/contract_restore [id:<id>]`: retoma sessao pausada (ou associa contrato especifico)
- `/contract_summary id:<contract_id>`: mostra o resumo de execucao de um contrato
- `/contract_session_summary`: resume a sessao atual
- `/contract_sessions`: lista sessoes abertas/pausadas do utilizador

## Fluxo do upload de contrato

Quando usas `/contract_upload` sem `conteudo`, o bot entra em modo de upload pendente para aquele utilizador/canal.

1. Envia uma ou varias mensagens com o conteudo do contrato.
2. Usa `/contract_upload_finish` para gravar.
3. Usa `/contract_upload_cancel` para abortar.

## Persistencia de dados

- Conversas normais: `data/conversations.json`
- Catalogo de contratos: `data/contracts.json`
- Resumos de execucao: `data/contract_summaries.json`
- Sessoes de contrato: `data/contract_sessions.json`

As sessoes de contrato agora sobrevivem a reinicios do bot.

## API HTTP

- `GET http://127.0.0.1:3000/status`

Exemplo:

```text
{"api":"api-ok","bot":"bot-ok"}
```

## IA e fallback

1. Tenta Gemini (`GEMINI_API_KEY`).
2. Em `429`, tenta OpenRouter (`OPENROUTER_API_KEY`).
3. Se ambos falharem, devolve erro no Discord.

## IA e fallback

1. Tenta usar Gemini com `GEMINI_API_KEY`.
2. Se receber `429`, tenta OpenRouter com `OPENROUTER_API_KEY`.
3. Se ambos falharem, devolve um erro no Discord.

## Notas de segurança

- Não publiques tokens ou chaves em screenshots, commits ou chats.
- Se uma chave for exposta, revoga-a e cria outra imediatamente.
- Nao publiques tokens/chaves em screenshots, commits ou chats.
- Se uma chave for exposta, revoga e cria outra.
