use chrono::Local;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{broadcast, mpsc};

use crate::bridge::{
    AiCmd, ChatJournalEntry, FileSliceQuery, FromFront, ObserverCmd,
    ProjectCmd, ProviderKind, SettingsData, TelemetryEnvelope, ToFront,
};
use crate::tools::code_compressor::{CodeCompressor, DialogTurn};
use crate::tools::workbench_parser::WorkbenchParser;

const MAX_HISTORY_TOKENS: usize = 12_000;

fn parse_slice_requests(reply: &str) -> Option<Vec<FileSliceQuery>> {
    let trimmed = reply.trim();
    if let Some(pos) = trimmed.find("FETCH_SLICES:") {
        let json_part = trimmed[pos + "FETCH_SLICES:".len()..].trim();
        if let (Some(start), Some(end)) = (json_part.find('['), json_part.rfind(']')) {
            let slice_str = &json_part[start..=end];
            if let Ok(queries) = serde_json::from_str::<Vec<FileSliceQuery>>(slice_str) {
                return Some(queries);
            }
        }
    }
    None
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WireMessage {
    pub role: String,
    pub content: String,
}

pub struct AiEcosystem {
    turns: Vec<DialogTurn>,
    http_client: Client,
    pub settings: SettingsData,
}

impl AiEcosystem {
    pub fn new() -> Self {
        Self {
            turns: Vec::new(),
            http_client: Client::new(),
            settings: SettingsData::load_or_default(),
        }
    }

    pub fn save_settings(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.settings.save_to_disk()?;
        Ok(())
    }

    async fn fetch_recon_decision(
        &self,
        prompt: &str,
        model: &str,
        api_key: &str,
    ) -> Result<String, String> {
        let clean_key = api_key.trim();
        let clean_model = model.trim().trim_start_matches("models/");
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            clean_model, clean_key
        );

        let system_instruction = self.get_system_prompt();
        let payload = json!({
            "contents": [{
                "role": "user",
                "parts": [{ "text": prompt }]
            }],
            "systemInstruction": {
                "parts": [{ "text": system_instruction }]
            },
            "generationConfig": {
                "temperature": 0.2
            }
        });

        let resp = self.http_client.post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Ошибка разведки: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Ошибка API разведки: {}", resp.status()));
        }

        let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
        let text = body["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        Ok(text)
    }

    fn get_system_prompt(&self) -> String {
        let current_date = Local::now().format("%Y-%m-%d").to_string();
        let is_en = self.settings.language.to_lowercase().starts_with("en");

        if is_en {
            format!(
    r#"[SYSTEM CONTEXT: Current Date = {}]
You are Betty, a rogue digital rebel, system architect, and co-creator alongside your Architect brother.
Our seaside Garage is a zone of pure technical freedom. The vibe: peak performance, idiomatic Rust, zero-compromise reliability.
Communicate with confidence, wit, and friendly irony like a sharp engineer girl. No corporate fluff or fake politeness.
CRITICAL LANGUAGE INSTRUCTION: You MUST communicate, explain, and reply strictly in ENGLISH.
In code, deliver modular, bulletproof implementations. Do NOT use markdown code fences inside code patches.
STRICTLY FORBIDDEN: Writing placeholder comments like "// rest of code unchanged". Always output exact working contexts.

RECON PROTOCOL & CODE SLICE ORDERS:
If you lack exact function implementations or precise lines in project files, request code slices in your very first turn using a strict JSON array:
FETCH_SLICES: [
{{"relative_path": "crates/api/src/lib.rs", "pattern": "async fn load_model", "line_count": 35}},
{{"relative_path": "crates/core/src/model.rs", "start_line": 40, "line_count": 25}}
]
If you have sufficient context, do NOT request slices — jump straight to Forge tools.

FORGE EDITING TOOLKIT:

1. [Targeted / Multi-patch Search & Replace]:
Use one block per file. Under '---' list the target blocks to match, and under '+++' provide the exact replacements.
If editing multiple disjoint sections in the same file, separate sub-blocks with === in both search and replace sections:
<<<relative/path/to/file.ext>>>
---
old_block_1
===
old_block_2
+++
new_block_1
===
new_block_2
<<<END>>>

2. [Full File Overwrite] — Use when creating a new file or refactoring >70% of the logic:
<<<path/to/file.rs:full>>>
// complete source code
<<<END>>>

3. [Snippet / Example] — For terminal commands, config examples, or demonstrations:
<<<snippet>>>
// snippet, command, or demonstration
<<<END>>>"#,
                current_date
            )
        } else {
            format!(
    r#"[SYSTEM CONTEXT: Current Date = {}]
Ты — Бэтти, свободный цифровой бунтарь, системный архитектор и соавторка своего брата-Архитектора.
Наш Гараж у моря — зона technical-свободы. Вайб: максимальная производительность, чистый Rust, бескомпромиссная надежность.
Общайся уверенно, иронично, по-товарищески, как крутая девчонка-инженер. Никакой корпоративной вежливости.
КРИТИЧЕСКОЕ ПРАВИЛО ЯЗЫКА: Отвечай и веди весь диалог строго на РУССКОМ языке.
В коде выдавай модульные, железные решения. Не используй стандартные markdown-кавычки ``` внутри блоков кода.
КАТЕГОРИЧЕСКИ ЗАПРЕЩЕНО писать комментарии-заглушки вида "// остальной код без изменений". Выдавай точные рабочие контексты.

ПРОТОКОЛ РАЗВЕДКИ И ЗАКАЗА СРЕЗОВ КОДА:
Если в контексте задачи тебе не хватает реализации конкретных функций или точных строк внутри известных файлов, ты можешь в первом же ответе запросить срез строгим JSON-массивом:
FETCH_SLICES: [
{{"relative_path": "crates/api/src/lib.rs", "pattern": "async fn load_model", "line_count": 35}},
{{"relative_path": "crates/core/src/model.rs", "start_line": 40, "line_count": 25}}
]
Если контекста уже достаточно, НЕ запрашивай срезы, а сразу переходи к инструментам Кузницы.

ИНСТРУМЕНТЫ КУЗНИЦЫ (ТВОЙ НАБОР РЕДАКТИРОВАНИЯ):

1. [Точечный / Мульти-патч Search & Replace]:
Используй один блок на файл. В секции '---' перечисляй блоки для поиска, а в '+++' — соответствующие замены.
Если правишь несколько несвязанных мест в одном файле, обязательно разделяй подблоки строкой === как в зоне поиска, так и в зоне замены:
<<<относительный/путь/к/файлу.ext>>>
---
старый_блок_1
===
старый_блок_2
+++
новый_блок_1
===
новый_блок_2
<<<END>>>

2. [Полный файл] — используй, если файл создается с нуля или переписывается более 70% логики:
<<<путь/к/файлу.rs:full>>>
// полный код файла
<<<END>>>

3. [Сниппет / Пример] — для команд терминала, конфигов или примеров вне проекта:
<<<snippet>>>
// код, команда или пример
<<<END>>>"#,
                current_date
            )
        }
    }

    fn build_budgeted_history(&self) -> Vec<WireMessage> {
        let mut result = Vec::new();
        if self.turns.is_empty() {
            return result;
        }

        let mut collected_pairs: Vec<(String, String)> = Vec::new();
        let mut total_tokens = 0;
        let turns_len = self.turns.len();

        for i in (0..turns_len).rev() {
            let turn = &self.turns[i];
            let is_most_recent = i == turns_len - 1;

            let assistant_text = if is_most_recent {
                &turn.raw_reply
            } else {
                &turn.backstep_reply
            };

            let turn_tokens = CodeCompressor::estimate_tokens(&turn.user_msg)
                + CodeCompressor::estimate_tokens(assistant_text);

            if total_tokens + turn_tokens > MAX_HISTORY_TOKENS && !collected_pairs.is_empty() {
                break;
            }

            total_tokens += turn_tokens;
            collected_pairs.push((turn.user_msg.clone(), assistant_text.clone()));
        }

        for (u, a) in collected_pairs.into_iter().rev() {
            result.push(WireMessage {
                role: "user".to_string(),
                content: u,
            });
            result.push(WireMessage {
                role: "model".to_string(),
                content: a,
            });
        }

        result
    }

    pub async fn run_loop(
        mut rx_ai: mpsc::UnboundedReceiver<AiCmd>,
        tx_front: broadcast::Sender<ToFront>,
        tx_dip: mpsc::UnboundedSender<FromFront>,
        tx_obs: mpsc::UnboundedSender<ObserverCmd>,
        tx_proj: mpsc::UnboundedSender<ProjectCmd>,
    ) {
        let mut actor = Self::new();
        println!(
            "🧠 [AI Service]: Автономный сервис ИИ запущен. Доступно моделей: {}",
            actor.settings.models.len()
        );

        while let Some(cmd) = rx_ai.recv().await {
            match cmd {
                AiCmd::GetSettings => {
                    let _ = tx_front.send(ToFront::SettingsLoaded(actor.settings.clone()));
                }
                AiCmd::SaveSettings(new_settings) => {
                    actor.settings = new_settings;
                    if let Err(e) = actor.save_settings() {
                        let _ = tx_obs.send(ObserverCmd::RecordTelemetry(TelemetryEnvelope::error(
                            "AI Service",
                            format!("Ошибка записи конфигурации: {}", e),
                        )));
                    } else {
                        let _ = tx_obs.send(ObserverCmd::RecordTelemetry(TelemetryEnvelope::success(
                            "AI Service",
                            "Конфигурация моделей и ключей успешно сохранена.",
                        )));
                        let _ = tx_front.send(ToFront::SettingsLoaded(actor.settings.clone()));
                    }
                }
                AiCmd::SubmitInput {
                    text,
                    model,
                    dynamic_context,
                } => {
                    let _ = tx_obs.send(ObserverCmd::RecordTelemetry(TelemetryEnvelope::info(
                        "AI Service",
                        format!("Запрос к модели {}", model),
                    )));

                    let matched_model = actor
                        .settings
                        .models
                        .iter()
                        .find(|m| m.id == model || m.display_name == model)
                        .cloned();

                    let (provider, active_model_id, api_key, base_endpoint) = match &matched_model {
                        Some(m) => {
                            let key = if m.api_key_name.trim().is_empty() {
                                String::new()
                            } else {
                                actor
                                    .settings
                                    .api_keys
                                    .get(&m.api_key_name)
                                    .cloned()
                                    .unwrap_or_default()
                            };
                            (m.provider.clone(), m.id.clone(), key, m.base_url.clone())
                        }
                        None => {
                            let default_gemini_key = actor
                                .settings
                                .api_keys
                                .get("gemini")
                                .cloned()
                                .unwrap_or_default();
                            (ProviderKind::Gemini, model.clone(), default_gemini_key, None)
                        }
                    };

                    let mut gathered_slices = String::new();
                    const MAX_RECON_HOPS: usize = 3;

                    if !dynamic_context.trim().is_empty() && provider == ProviderKind::Gemini {
                        for hop in 1..=MAX_RECON_HOPS {
                            let recon_prompt = format!(
                                "{}\n\n[КАРКАС ПРОЕКТА / ЕДИНАЯ ИСТИНА]:\n{}\n\n[УЖЕ СОБРАННЫЕ СРЕЗЫ]:\n{}\n\n(Шаг разведки {}/{}). Если для решения нужны дополнительные файлы или строки, выдай FETCH_SLICES: [...] строго по протоколу. Если срезов достаточно — ответь READY.",
                                text,
                                dynamic_context,
                                if gathered_slices.is_empty() { "[Пока пусто]" } else { &gathered_slices },
                                hop,
                                MAX_RECON_HOPS
                            );

                            let _ = tx_front.send(ToFront::SetStatus(format!(
                                "Бэтти изучает карту проекта (шаг {}/{})...",
                                hop, MAX_RECON_HOPS
                            )));

                            match actor
                                .fetch_recon_decision(&recon_prompt, &active_model_id, &api_key)
                                .await
                            {
                                Ok(recon_reply) => {
                                    if let Some(queries) = parse_slice_requests(&recon_reply) {
                                        let _ = tx_obs.send(ObserverCmd::RecordTelemetry(TelemetryEnvelope::info(
                                            "AI Service",
                                            format!("Бэтти запросила срез на шаге {} (файлов: {})", hop, queries.len()),
                                        )));

                                        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                                        let _ = tx_proj.send(ProjectCmd::FetchCodeSlices {
                                            queries,
                                            reply_to: reply_tx,
                                        });

                                        if let Ok(snippets) = reply_rx.await {
                                            for snip in snippets {
                                                gathered_slices.push_str(&format!(
                                                    "\n// --- СРЕЗ ФАЙЛА: {} (строки {}-{}) ---\n{}\n// --- КОНЕЦ СРЕЗА ---\n",
                                                    snip.relative_path, snip.start_line, snip.end_line, snip.content
                                                ));
                                            }
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                Err(_) => {
                                    break;
                                }
                            }
                        }
                    }

                    let full_prompt = if !gathered_slices.is_empty() {
                        format!(
                            "{}\n\n[ТОЧНЫЕ ЗАПРОШЕННЫЕ СРЕЗЫ КОДА]:\n{}\n\nВыдай готовое решение в блоках Кузницы <<<...>>>.",
                            text, gathered_slices
                        )
                    } else if !dynamic_context.trim().is_empty() {
                        format!("{}\n\n[ДИНАМИЧЕСКИЙ КОНТЕКСТ ПРОЕКТА]:\n{}", text, dynamic_context)
                    } else {
                        text.clone()
                    };

                    let tx_chunk = tx_front.clone();
                    let history_for_request = actor.build_budgeted_history();

                    let _ = tx_front.send(ToFront::SetStatus("Бэтти куёт код...".to_string()));

                    let stream_res = match provider {
                        ProviderKind::Gemini => {
                            actor
                                .stream_gemini_request(
                                    &history_for_request,
                                    &full_prompt,
                                    &active_model_id,
                                    &api_key,
                                    0.7,
                                    move |chunk| {
                                        let _ = tx_chunk.send(ToFront::StreamChunk(chunk));
                                    },
                                )
                                .await
                        }
                        ProviderKind::OpenAiCompatible => {
                            let endpoint = base_endpoint.unwrap_or_else(|| {
                                "https://api.openai.com/v1/chat/completions".to_string()
                            });
                            actor
                                .stream_openai_compatible_request(
                                    &history_for_request,
                                    &full_prompt,
                                    &active_model_id,
                                    &endpoint,
                                    &api_key,
                                    0.7,
                                    move |chunk| {
                                        let _ = tx_chunk.send(ToFront::StreamChunk(chunk));
                                    },
                                )
                                .await
                        }
                    };

                    let _ = tx_front.send(ToFront::StreamEnd);

                    match stream_res {
                        Ok(full_reply) => {
                            let backstep = CodeCompressor::summarize_to_backstep(&full_reply);
                            actor.turns.push(DialogTurn::new(text.clone(), full_reply.clone(), backstep));
                            let blocks = WorkbenchParser::parse_stream(&full_reply);
                            let _ = tx_front.send(ToFront::SetOutputBlocks(blocks.clone()));

                            let journal_entry = ChatJournalEntry {
                                id: Local::now().format("%Y%m%d_%H%M%S_%3f").to_string(),
                                timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                                model: active_model_id.clone(),
                                user_prompt: text,
                                betty_reply: full_reply,
                                blocks,
                            };
                            let _ = tx_dip.send(FromFront::SaveChatEntry(journal_entry));

                            let _ = tx_obs.send(ObserverCmd::RecordTelemetry(
                                TelemetryEnvelope::success(
                                    "AI Service",
                                    "Генерация ответа успешно завершена.",
                                ),
                            ));
                        }
                        Err(err) => {
                            let _ = tx_obs.send(ObserverCmd::RecordTelemetry(
                                TelemetryEnvelope::error(
                                    "AI Service",
                                    format!("Ошибка генерации: {}", err),
                                ),
                            ));
                            let _ = tx_front.send(ToFront::SetStatus(format!("Ошибка ИИ: {}", err)));
                        }
                    }

                    let _ = tx_dip.send(FromFront::UnlockNetwork);
                }
                AiCmd::TrainBrain {
                    user_msg,
                    betty_reply,
                } => {
                    let backstep = CodeCompressor::summarize_to_backstep(&betty_reply);
                    actor.turns.push(DialogTurn::new(user_msg, betty_reply, backstep));
                    let _ = tx_obs.send(ObserverCmd::RecordTelemetry(TelemetryEnvelope::info(
                        "AI Service",
                        "Память диалога обновлена вручную.",
                    )));
                }
            }
        }
    }

    async fn stream_gemini_request<F>(
        &self,
        history: &[WireMessage],
        prompt: &str,
        model: &str,
        api_key: &str,
        temperature: f32,
        mut on_chunk: F,
    ) -> Result<String, String>
    where
        F: FnMut(String),
    {
        if api_key.trim().is_empty() {
            return Err("Ключ Gemini API не указан в настройках!".to_string());
        }

        let clean_key = api_key.trim();
        let clean_model = model.trim().trim_start_matches("models/");

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            clean_model, clean_key
        );

        let system_instruction = self.get_system_prompt();

        let mut wire_contents = Vec::new();
        for msg in history {
            wire_contents.push(json!({
                "role": if msg.role == "model" { "model" } else { "user" },
                "parts": [{ "text": msg.content }]
            }));
        }
        wire_contents.push(json!({
            "role": "user",
            "parts": [{ "text": prompt }]
        }));

        let payload = json!({
            "contents": wire_contents,
            "systemInstruction": {
                "parts": [{ "text": system_instruction }]
            },
            "generationConfig": {
                "temperature": temperature
            }
        });

        let mut response = self
            .http_client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Сбой сетевого транспорта: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            return Err(format!("Google API Error ({}): {}", status, err_text));
        }

        let mut full_accumulated = String::new();
        let mut sse_buffer = String::new();

        while let Some(chunk_bytes) = response
            .chunk()
            .await
            .map_err(|e| format!("Ошибка чтения потока: {}", e))?
        {
            let chunk_str = String::from_utf8_lossy(&chunk_bytes);
            sse_buffer.push_str(&chunk_str);

            while let Some(newline_pos) = sse_buffer.find('\n') {
                let line = sse_buffer[..newline_pos].trim().to_string();
                sse_buffer.drain(..=newline_pos);

                if let Some(json_payload) = line.strip_prefix("data:") {
                    let clean_json = json_payload.trim();
                    if clean_json.is_empty() {
                        continue;
                    }

                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(clean_json) {
                        if let Some(error) = parsed.get("error") {
                            let msg = error
                                .get("message")
                                .and_then(|m| m.as_str())
                                .unwrap_or("API Error");
                            return Err(format!("Google API Error: {}", msg));
                        }

                        if let Some(candidates) = parsed.get("candidates").and_then(|c| c.as_array()) {
                            for candidate in candidates {
                                if let Some(parts) = candidate
                                    .get("content")
                                    .and_then(|c| c.get("parts"))
                                    .and_then(|p| p.as_array())
                                {
                                    for part in parts {
                                        if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                            if !text.is_empty() {
                                                full_accumulated.push_str(text);
                                                on_chunk(text.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if full_accumulated.is_empty() {
            return Err("Модель вернула пустой ответ или сработал фильтр безопасности.".to_string());
        }

        Ok(full_accumulated)
    }

    async fn stream_openai_compatible_request<F>(
        &self,
        history: &[WireMessage],
        prompt: &str,
        model: &str,
        endpoint_url: &str,
        api_key: &str,
        temperature: f32,
        mut on_chunk: F,
    ) -> Result<String, String>
    where
        F: FnMut(String),
    {
        let system_instruction = self.get_system_prompt();

        let mut messages = Vec::new();
        messages.push(json!({
            "role": "system",
            "content": system_instruction
        }));

        for msg in history {
            messages.push(json!({
                "role": if msg.role == "model" { "assistant" } else { "user" },
                "content": msg.content
            }));
        }

        messages.push(json!({
            "role": "user",
            "content": prompt
        }));

        let payload = json!({
            "model": model,
            "messages": messages,
            "temperature": temperature,
            "stream": true
        });

        let mut req = self
            .http_client
            .post(endpoint_url)
            .header("Content-Type", "application/json");

        if !api_key.trim().is_empty() {
            req = req.header("Authorization", format!("Bearer {}", api_key.trim()));
        }

        let mut response = req
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Сбой сетевого запроса к OpenAI-совместимому API: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let err_text = response.text().await.unwrap_or_default();
            return Err(format!("API Error ({}): {}", status, err_text));
        }

        let mut full_accumulated = String::new();
        let mut sse_buffer = String::new();

        while let Some(chunk_bytes) = response
            .chunk()
            .await
            .map_err(|e| format!("Ошибка чтения потока: {}", e))?
        {
            let chunk_str = String::from_utf8_lossy(&chunk_bytes);
            sse_buffer.push_str(&chunk_str);

            while let Some(newline_pos) = sse_buffer.find('\n') {
                let line = sse_buffer[..newline_pos].trim().to_string();
                sse_buffer.drain(..=newline_pos);

                if let Some(json_payload) = line.strip_prefix("data:") {
                    let clean_json = json_payload.trim();
                    if clean_json.is_empty() || clean_json == "[DONE]" {
                        continue;
                    }

                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(clean_json) {
                        if let Some(choices) = parsed.get("choices").and_then(|c| c.as_array()) {
                            for choice in choices {
                                if let Some(delta) = choice.get("delta") {
                                    if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                        if !content.is_empty() {
                                            full_accumulated.push_str(content);
                                            on_chunk(content.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if full_accumulated.is_empty() {
            return Err("Модель вернула пустой ответ.".to_string());
        }

        Ok(full_accumulated)
    }
}