// ============================================================
// loop_runner.rs - Agent 執行迴圈模組
// 對應 Python 版的 core/loop.py
//
// 注意：Rust 中 `loop` 是保留關鍵字，所以這個檔案叫 loop_runner。
//
// 職責：
//   1. 呼叫大模型取得串流回應
//   2. 判斷是純文字回應還是 Tool Call
//   3. 解析 Tool Call 並執行工具
//   4. 將工具結果注入上下文，繼續呼叫大模型
//   5. 透過 channel 把所有事件傳給外部（用於 SSE 推送）
// ============================================================

use std::collections::HashMap;
use std::sync::Arc;

use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionMessageToolCall, ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestToolMessageArgs, ChatCompletionRequestUserMessageArgs,
        ChatCompletionTool, ChatCompletionToolType, CreateChatCompletionRequestArgs,
        FunctionCall, FunctionObject,
    },
    Client,
};
use futures::StreamExt;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tracing::{info, warn};

use crate::tools::ToolRegistry;

// -----------------------------------------------------------
// Rust 學習筆記：型別別名（type alias）
// -----------------------------------------------------------
// `type EventSender = mpsc::Sender<Value>` 讓我們可以用
// EventSender 代替每次都寫 mpsc::Sender<Value>。
// -----------------------------------------------------------
pub type EventSender = mpsc::Sender<Value>;

pub struct AgentLoop {
    client: Client<OpenAIConfig>,
    model: String,
}

// -----------------------------------------------------------
// Rust 學習筆記：Tool Call 串流累積緩衝區
// -----------------------------------------------------------
// LLM 的 tool call 資訊是分片串流過來的，需要逐片拼接。
// 我們用這個 struct 來累積每個 tool call 的資料。
// -----------------------------------------------------------
struct ToolCallAccumulator {
    id: String,
    name: String,
    arguments: String,
}

impl AgentLoop {
    pub fn new(client: Client<OpenAIConfig>, model: String) -> Self {
        Self { client, model }
    }

    // -----------------------------------------------------------
    // Rust 學習筆記：用 channel 取代 AsyncGenerator
    // -----------------------------------------------------------
    // Python 有 AsyncGenerator（async for ... yield）。
    // Rust stable 目前推薦用 mpsc::channel 來替代：
    //   - tx（Sender）：在 loop 裡 tx.send(event).await
    //   - rx（Receiver）：在 axum 裡包裝成 SSE Stream
    //
    // 回傳值 Vec<Value> 是本輪新產生的訊息（供 agent 存入記憶）。
    //
    // `Arc<ToolRegistry>` 是「原子引用計數指標」（Atomic Reference Count）：
    //   - Arc 讓多個地方可以共享同一份 ToolRegistry
    //   - 相當於 Python 把物件傳入函式（Python 預設就是引用語義）
    //   - 加上 Arc 是因為跨 async 邊界需要 Rust 明確聲明共享所有權
    // -----------------------------------------------------------
    pub async fn run(
        &self,
        messages_payload: Vec<Value>,
        tool_registry: Arc<ToolRegistry>,
        tx: &EventSender,
    ) -> Vec<Value> {
        let mut current_messages = messages_payload.clone();
        let max_iterations = 10;
        let tools_def = tool_registry.get_definitions();
        let openai_tools = build_openai_tools(&tools_def);

        for iteration in 0..max_iterations {
            info!(
                "[AgentLoop] 發起第 {} 輪請求，攜帶 {} 條歷史",
                iteration + 1,
                current_messages.len()
            );

            // 清理訊息（移除空 tool_calls、空 content）
            let cleaned = clean_messages(&current_messages);

            // 將存儲用的 JSON 格式轉換成 async-openai 的強型別
            let openai_messages: Vec<ChatCompletionRequestMessage> = cleaned
                .iter()
                .filter_map(json_to_chat_message)
                .collect();

            // -----------------------------------------------------------
            // Rust 學習筆記：Builder Pattern
            // -----------------------------------------------------------
            // async-openai 使用 Builder pattern 建立請求：
            //   CreateChatCompletionRequestArgs::default()
            //       .model(...)       ← 設定欄位
            //       .messages(...)    ← 設定欄位
            //       .build()          ← 建立並驗證
            //       .unwrap()         ← 取出值（或處理錯誤）
            //
            // 這等同於 Python 的：
            //   {"model": ..., "messages": ..., ...}
            // 但 Rust 在編譯期就能檢查型別是否正確。
            // -----------------------------------------------------------
            let mut request_builder = CreateChatCompletionRequestArgs::default();
            request_builder
                .model(&self.model)
                .messages(openai_messages)
                .stream(true);

            if !openai_tools.is_empty() {
                request_builder.tools(openai_tools.clone());
            }

            let request = match request_builder.build() {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(json!({"type": "error", "content": e.to_string()}))
                        .await;
                    break;
                }
            };

            // 呼叫 API 取得串流回應
            let stream = match self.client.chat().create_stream(request).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx
                        .send(json!({
                            "type": "error",
                            "content": format!("呼叫大模型 API 失敗: {}", e)
                        }))
                        .await;
                    break;
                }
            };

            // -----------------------------------------------------------
            // Rust 學習筆記：pin_mut! 和 Stream
            // -----------------------------------------------------------
            // `pin_mut!` 把 stream 固定在記憶體中（Pin）。
            // 某些非同步迭代器需要被 Pin 才能安全使用 .next().await。
            // 這是 Rust 的記憶體安全保障機制，Python 不需要這個概念。
            // -----------------------------------------------------------
            futures::pin_mut!(stream);

            let mut assistant_content = String::new();
            let mut tool_call_buffer: HashMap<u32, ToolCallAccumulator> = HashMap::new();

            // 遍歷串流的每一個片段
            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        warn!("串流錯誤: {}", e);
                        continue;
                    }
                };

                // 記錄 Token 消耗
                if let Some(usage) = &chunk.usage {
                    let _ = tx
                        .send(json!({
                            "type": "token_usage",
                            "prompt_tokens": usage.prompt_tokens,
                            "completion_tokens": usage.completion_tokens,
                            "total_tokens": usage.total_tokens
                        }))
                        .await;
                }

                for choice in &chunk.choices {
                    let delta = &choice.delta;

                    // 情況 1: 普通文字輸出
                    if let Some(content) = &delta.content {
                        if !content.is_empty() {
                            assistant_content.push_str(content);
                            let _ = tx
                                .send(json!({
                                    "type": "text_delta",
                                    "content": content
                                }))
                                .await;
                        }
                    }

                    // 情況 2: Tool Call 串流片段 → 累積到 buffer
                    if let Some(tool_calls) = &delta.tool_calls {
                        for tc in tool_calls {
                            let idx = tc.index;
                            let acc = tool_call_buffer
                                .entry(idx)
                                .or_insert_with(|| ToolCallAccumulator {
                                    id: String::new(),
                                    name: String::new(),
                                    arguments: String::new(),
                                });

                            if let Some(id) = &tc.id {
                                acc.id.push_str(id);
                            }
                            if let Some(func) = &tc.function {
                                if let Some(name) = &func.name {
                                    acc.name.push_str(name);
                                }
                                if let Some(args) = &func.arguments {
                                    acc.arguments.push_str(args);
                                }
                            }
                        }
                    }
                }
            }

            // 將累積的 tool_calls 整理成 JSON 陣列
            let tool_calls_json: Vec<Value> = {
                let mut sorted_keys: Vec<u32> = tool_call_buffer.keys().cloned().collect();
                sorted_keys.sort();
                sorted_keys
                    .iter()
                    .map(|k| {
                        let acc = &tool_call_buffer[k];
                        json!({
                            "id": acc.id,
                            "type": "function",
                            "function": {
                                "name": acc.name,
                                "arguments": acc.arguments
                            }
                        })
                    })
                    .collect()
            };

            // 組裝 assistant 訊息
            let assistant_msg = if !tool_calls_json.is_empty() {
                json!({
                    "role": "assistant",
                    "content": if assistant_content.is_empty() { Value::Null } else { json!(assistant_content) },
                    "tool_calls": tool_calls_json
                })
            } else {
                json!({
                    "role": "assistant",
                    "content": if assistant_content.is_empty() { Value::Null } else { json!(assistant_content) }
                })
            };

            current_messages.push(assistant_msg.clone());

            // 如果沒有 tool_calls，本輪結束
            let has_tool_calls = assistant_msg
                .get("tool_calls")
                .and_then(|tc| tc.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);

            if !has_tool_calls {
                break;
            }

            // 執行每個 tool call
            if let Some(tcs) = assistant_msg.get("tool_calls").and_then(|tc| tc.as_array()) {
                for tc in tcs {
                    let tool_id = tc
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let tool_name = tc
                        .get("function")
                        .and_then(|f| f.get("name"))
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let tool_args = tc
                        .get("function")
                        .and_then(|f| f.get("arguments"))
                        .and_then(|a| a.as_str())
                        .unwrap_or("{}")
                        .to_string();

                    // 通知前端開始執行工具
                    let _ = tx
                        .send(json!({
                            "type": "tool_call_start",
                            "id": tool_id,
                            "name": tool_name,
                            "arguments": tool_args
                        }))
                        .await;

                    info!("執行工具 '{}' 參數: {}", tool_name, &tool_args);

                    // 實際執行工具
                    let result = tool_registry.execute(&tool_name, &tool_args).await;

                    info!("執行結果: {}...", &result.chars().take(100).collect::<String>());

                    // 通知前端工具執行完畢
                    let summary = if result.len() > 100 {
                        format!("{}...", &result[..100])
                    } else {
                        result.clone()
                    };

                    let _ = tx
                        .send(json!({
                            "type": "tool_call_end",
                            "id": tool_id,
                            "name": tool_name,
                            "result_summary": summary
                        }))
                        .await;

                    // 把工具結果注入 messages，下一輪繼續
                    current_messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_id,
                        "name": tool_name,
                        "content": result
                    }));
                }
            }
        }

        // 返回本輪新產生的訊息（不包含傳入的舊訊息）
        current_messages[messages_payload.len()..].to_vec()
    }
}

// ============================================================
// 輔助函式
// ============================================================

/// 將工具定義從 serde_json::Value 轉換成 async-openai 型別
fn build_openai_tools(tools_def: &[Value]) -> Vec<ChatCompletionTool> {
    tools_def
        .iter()
        .filter_map(|t| {
            let func = t.get("function")?;
            let name = func.get("name")?.as_str()?.to_string();
            let description = func
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let parameters = func.get("parameters").cloned().unwrap_or(json!({}));

            Some(ChatCompletionTool {
                r#type: ChatCompletionToolType::Function,
                function: FunctionObject {
                    name,
                    description: Some(description),
                    parameters: Some(parameters),
                    strict: None,
                },
            })
        })
        .collect()
}

/// 清理訊息列表（移除空 tool_calls、處理空 content）
fn clean_messages(messages: &[Value]) -> Vec<Value> {
    messages
        .iter()
        .map(|m| {
            let mut msg = m.clone();
            if let Some(obj) = msg.as_object_mut() {
                if let Some(tc) = obj.get("tool_calls") {
                    if tc.as_array().map(|a| a.is_empty()).unwrap_or(false) {
                        obj.remove("tool_calls");
                    }
                }
                if obj.get("content").and_then(|c| c.as_str()) == Some("") {
                    obj.insert("content".to_string(), Value::Null);
                }
            }
            msg
        })
        .collect()
}

/// 將存儲用的 JSON 訊息轉換成 async-openai 的強型別
fn json_to_chat_message(msg: &Value) -> Option<ChatCompletionRequestMessage> {
    let role = msg.get("role")?.as_str()?;

    match role {
        "system" => {
            let content = msg.get("content")?.as_str()?.to_string();
            Some(ChatCompletionRequestMessage::System(
                ChatCompletionRequestSystemMessageArgs::default()
                    .content(content)
                    .build()
                    .ok()?,
            ))
        }
        "user" => {
            let content = msg.get("content")?.as_str()?.to_string();
            Some(ChatCompletionRequestMessage::User(
                ChatCompletionRequestUserMessageArgs::default()
                    .content(content)
                    .build()
                    .ok()?,
            ))
        }
        "assistant" => {
            let content = msg
                .get("content")
                .and_then(|c| c.as_str())
                .map(|s| s.to_string());

            let tool_calls: Option<Vec<ChatCompletionMessageToolCall>> = msg
                .get("tool_calls")
                .and_then(|tcs| tcs.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|tc| {
                            let id = tc.get("id")?.as_str()?.to_string();
                            let func = tc.get("function")?;
                            let name = func.get("name")?.as_str()?.to_string();
                            let arguments = func
                                .get("arguments")
                                .and_then(|a| a.as_str())
                                .unwrap_or("{}")
                                .to_string();
                            Some(ChatCompletionMessageToolCall {
                                id,
                                r#type: ChatCompletionToolType::Function,
                                function: FunctionCall { name, arguments },
                            })
                        })
                        .collect()
                });

            let mut builder = ChatCompletionRequestAssistantMessageArgs::default();
            if let Some(c) = content {
                builder.content(c);
            }
            if let Some(tc) = tool_calls {
                if !tc.is_empty() {
                    builder.tool_calls(tc);
                }
            }
            Some(ChatCompletionRequestMessage::Assistant(
                builder.build().ok()?,
            ))
        }
        "tool" => {
            let tool_call_id = msg.get("tool_call_id")?.as_str()?.to_string();
            let content = msg.get("content")?.as_str()?.to_string();
            Some(ChatCompletionRequestMessage::Tool(
                ChatCompletionRequestToolMessageArgs::default()
                    .tool_call_id(tool_call_id)
                    .content(content)
                    .build()
                    .ok()?,
            ))
        }
        _ => None,
    }
}
