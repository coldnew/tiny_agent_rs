// ============================================================
// agent.rs - TinyAgent 高層封裝模組
// 對應 Python 版的 core/agent.py
//
// 職責：
//   1. 組合所有子模組（Memory、Skills、Context、Loop）
//   2. 提供對外的 chat_stream 介面
//   3. 管理會話狀態
// ============================================================

use std::sync::Arc;

use anyhow::Result;
use async_openai::{config::OpenAIConfig, Client};
use serde_json::{json, Value};
use tokio::sync::Mutex;

use crate::context::ContextBuilder;
use crate::loop_runner::{AgentLoop, EventSender};
use crate::memory::MemoryStore;
use crate::skills::SkillsLoader;
use crate::tools::ToolRegistry;

// -----------------------------------------------------------
// Rust 學習筆記：Arc<Mutex<T>> - 跨執行緒共享可變狀態
// -----------------------------------------------------------
// Python 的 GIL 讓我們可以放心地從多個地方修改同一個物件。
// Rust 沒有 GIL，需要明確地同步。
//
// `Arc<Mutex<T>>` 是 Rust 中最常見的共享可變狀態模式：
//   - Arc（Atomic Reference Count）：讓多個地方能共享所有權
//   - Mutex（互斥鎖）：確保同一時間只有一個地方可以修改資料
//
// 在 axum 的 State 中，我們用 Arc<Mutex<TinyAgent>> 來讓
// 多個請求共享同一個 Agent 實例。
// -----------------------------------------------------------
pub struct TinyAgent {
    _workspace_dir: String,
    pub memory: MemoryStore,
    pub skills: SkillsLoader,
    tool_registry: Arc<ToolRegistry>,
    context: ContextBuilder,
    agent_loop: AgentLoop,
}

// -----------------------------------------------------------
// Rust 學習筆記：AppState 包裝
// -----------------------------------------------------------
// axum 的 State 需要 Clone，但 TinyAgent 不能 Clone（因為包含非 Clone 欄位）。
// 所以我們用 Arc<Mutex<TinyAgent>> 來包裝，Arc 本身是可以 Clone 的。
// -----------------------------------------------------------
#[derive(Clone)]
pub struct AppState {
    pub agent: Arc<Mutex<TinyAgent>>,
}

impl TinyAgent {
    // -----------------------------------------------------------
    // Rust 學習筆記：async 建構函式
    // -----------------------------------------------------------
    // Python:  def __init__(self, ...):
    //              self.memory = MemoryStore(...)
    //
    // Rust:    pub async fn new(...) -> Result<Self> {
    //              let memory = MemoryStore::new(...).await?;
    //              ...
    //              Ok(Self { memory, ... })
    //          }
    //
    // Rust 的 struct 沒有 __init__，慣例是提供 `new` 關聯函式。
    // 因為 MemoryStore::new 是 async 的，所以 TinyAgent::new 也必須是 async。
    // -----------------------------------------------------------
    pub async fn new(
        workspace_dir: &str,
        api_key: &str,
        base_url: Option<&str>,
        model: &str,
    ) -> Result<Self> {
        // 確保工作目錄存在
        tokio::fs::create_dir_all(workspace_dir).await?;

        // 初始化 OpenAI 客戶端
        // -----------------------------------------------------------
        // Rust 學習筆記：條件建構（if let）
        // -----------------------------------------------------------
        // Python:  if base_url:
        //              api_kwargs["base_url"] = base_url
        //
        // Rust 中我們用 Builder pattern 加上條件式：
        //   let config = if let Some(url) = base_url { ... } else { ... }
        // -----------------------------------------------------------
        let config = if let Some(url) = base_url {
            OpenAIConfig::new()
                .with_api_key(api_key)
                .with_api_base(url)
        } else {
            OpenAIConfig::new().with_api_key(api_key)
        };

        let client = Client::with_config(config);

        // 初始化各子模組
        let memory = MemoryStore::new(workspace_dir, "default").await?;
        let skills = SkillsLoader::new(workspace_dir).await;
        // Arc::new 讓多個地方可以共享 ToolRegistry（不用複製整個物件）
        let tool_registry = Arc::new(ToolRegistry::new());
        let context = ContextBuilder::new(workspace_dir);
        let agent_loop = AgentLoop::new(client, model.to_string());

        Ok(Self {
            _workspace_dir: workspace_dir.to_string(),
            memory,
            skills,
            tool_registry,
            context,
            agent_loop,
        })
    }

    // -----------------------------------------------------------
    // Rust 學習筆記：用 channel 實作事件串流
    // -----------------------------------------------------------
    // Python:  async def chat_stream(self, user_message) -> AsyncGenerator:
    //              async for event in self.loop.run(messages):
    //                  yield event
    //
    // Rust:    pub async fn chat_stream(&mut self, user_message, tx)
    //              // 透過 tx.send(event).await 傳送事件
    //
    // 呼叫者（main.rs）會建立 channel，把 tx 傳進來，
    // 自己用 rx 接收事件並包裝成 SSE。
    // -----------------------------------------------------------
    pub async fn chat_stream(&mut self, user_message: String, tx: EventSender) {
        // 1. 組裝發往大模型的訊息 Payload
        let messages_payload = self
            .context
            .build_messages(&user_message, &self.memory, &self.skills)
            .await;

        // 把用戶訊息加入記憶
        self.memory
            .add_message(json!({
                "role": "user",
                "content": user_message
            }))
            .await;

        // 2. 執行 Agent 迴圈（串流事件透過 tx 傳出去）
        let new_messages = self
            .agent_loop
            .run(messages_payload, Arc::clone(&self.tool_registry), &tx)
            .await;

        // 3. 處理本輪產生的事件（存入記憶等）
        // 注意：新訊息中的 token_usage 和 turn_end 在 loop 內部已處理過，
        // 這裡只需要把真實的 assistant/tool 訊息存入記憶。
        for msg in &new_messages {
            if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
                if role != "user" {
                    self.memory.add_message(msg.clone()).await;
                }
            }
        }

        // 通知前端本輪結束
        let _ = tx
            .send(json!({
                "type": "turn_end",
                "new_messages": new_messages
            }))
            .await;
    }

    pub fn get_skills_summary(&self) -> Vec<Value> {
        self.skills.get_skills_summary()
    }

    pub fn get_tools_summary(&self) -> Vec<Value> {
        self.tool_registry.get_tool_summaries()
    }

    pub async fn reload_skills(&mut self) {
        self.skills.load_all_skills().await;
    }

    pub fn get_messages(&self) -> &[Value] {
        &self.memory.messages
    }

    pub fn get_tokens(&self) -> &crate::memory::Tokens {
        self.memory.get_tokens()
    }

    pub async fn clear_memory(&mut self) {
        self.memory.clear_history().await;
    }
}
