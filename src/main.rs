// ============================================================
// main.rs - Web 伺服器入口
// 對應 Python 版的 app.py
//
// 職責：
//   1. 載入設定
//   2. 初始化 TinyAgent
//   3. 定義 HTTP API 路由
//   4. 啟動 axum Web 伺服器
// ============================================================

// -----------------------------------------------------------
// Rust 學習筆記：模組宣告
// -----------------------------------------------------------
// Rust 的每個檔案預設是獨立的模組。
// 要在 main.rs 裡使用其他檔案，必須先用 `mod` 宣告：
//   mod memory;   → 載入 src/memory.rs
//   mod tools;    → 載入 src/tools.rs
//   ... 以此類推
//
// Python 不需要這個步驟，因為 import 時會自動找到檔案。
// -----------------------------------------------------------
mod agent;
mod context;
mod loop_runner;
mod memory;
mod skills;
mod tools;

use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;

use agent::{AppState, TinyAgent};
use anyhow::Result;
use axum::{
    extract::{Multipart, Path as AxumPath, State},
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json,
    },
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{mpsc, Mutex};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tower_http::services::ServeDir;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// ============================================================
// 設定檔結構
// ============================================================

// -----------------------------------------------------------
// Rust 學習筆記：#[derive(Deserialize)] 自動產生 YAML 解析
// -----------------------------------------------------------
// Python:  config = yaml.safe_load(f)
//
// Rust:    #[derive(Deserialize)]
//          struct Config { llm: LlmConfig }
//          let config: Config = serde_yaml::from_str(&content)?;
//
// Rust 的 serde 框架可以從 derive 巨集自動生成序列化/反序列化程式碼。
// `Option<String>` 表示可能沒有這個欄位（對應 YAML 中的可選項）。
// -----------------------------------------------------------
#[derive(Debug, Deserialize, Default)]
struct Config {
    #[serde(default)]
    llm: LlmConfig,
}

#[derive(Debug, Deserialize, Default)]
struct LlmConfig {
    api_key: Option<String>,
    model: Option<String>,
    base_url: Option<String>,
}

fn load_config() -> Config {
    let config_path = "config.yaml";
    if Path::new(config_path).exists() {
        match std::fs::read_to_string(config_path) {
            Ok(content) => serde_yaml::from_str(&content).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    } else {
        Config::default()
    }
}

// ============================================================
// 請求/回應型別
// ============================================================

// -----------------------------------------------------------
// Rust 學習筆記：請求體反序列化
// -----------------------------------------------------------
// Python (FastAPI):
//   class ChatRequest(BaseModel):
//       message: str
//
// Rust (axum + serde):
//   #[derive(Deserialize)]
//   struct ChatRequest { message: String }
//
// axum 的 `Json(req): Json<ChatRequest>` 自動解析 JSON body。
// -----------------------------------------------------------
#[derive(Deserialize)]
struct ChatRequest {
    message: String,
}

// ============================================================
// main 函式 - 程式入口點
// ============================================================

// -----------------------------------------------------------
// Rust 學習筆記：#[tokio::main] 巨集
// -----------------------------------------------------------
// Python 的 asyncio.run(main()) 需要手動呼叫。
// Rust 用 #[tokio::main] 巨集自動設定非同步執行時。
//
// `async fn main()` 本身就是一個非同步函式，
// 這在 Rust 中需要 tokio 執行時的支援。
// -----------------------------------------------------------
#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日誌系統
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    // 載入設定
    let config = load_config();
    let llm = config.llm;

    let api_key = llm
        .api_key
        .or_else(|| std::env::var("OPENAI_API_KEY").ok())
        .expect("必須提供 api_key（在 config.yaml 或環境變數 OPENAI_API_KEY 中）");

    let model = llm.model.unwrap_or_else(|| "gpt-4o-mini".to_string());
    let base_url = llm.base_url.as_deref();

    let workspace_path = "./workspace";

    // 建立輸出目錄
    tokio::fs::create_dir_all(format!("{}/outputs", workspace_path)).await?;

    // 初始化 TinyAgent
    let agent = TinyAgent::new(workspace_path, &api_key, base_url, &model).await?;

    // -----------------------------------------------------------
    // Rust 學習筆記：Arc<Mutex<T>> 共享狀態
    // -----------------------------------------------------------
    // axum 的每個請求都在獨立的 task 中執行，
    // 但它們需要共享同一個 TinyAgent 實例。
    //
    // Arc::new(Mutex::new(agent)) 讓多個請求都能安全地存取 agent：
    //   - Arc：多個 task 共享所有權
    //   - Mutex：確保同一時間只有一個 task 可以修改 agent
    //
    // 等同於 Python 中把 agent 放在模組頂層（Python 有 GIL 保護）。
    // -----------------------------------------------------------
    let state = AppState {
        agent: Arc::new(Mutex::new(agent)),
    };

    // -----------------------------------------------------------
    // Rust 學習筆記：axum 路由定義
    // -----------------------------------------------------------
    // Python (FastAPI):
    //   @app.get("/")
    //   async def root(): ...
    //
    //   @app.post("/api/chat")
    //   async def chat_endpoint(req: ChatRequest): ...
    //
    // Rust (axum):
    //   let app = Router::new()
    //       .route("/", get(root))
    //       .route("/api/chat", post(chat_endpoint))
    //       .with_state(state);
    //
    // axum 的路由是函式指標，不是裝飾器。
    // `with_state(state)` 把共享狀態注入所有路由。
    // -----------------------------------------------------------
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/api/chat", post(chat_handler))
        .route("/api/status", get(status_handler))
        .route("/api/memory", get(memory_handler))
        .route("/api/history", get(history_handler))
        .route("/api/outputs", get(list_outputs_handler))
        .route("/api/outputs/:filename", delete(delete_output_handler))
        .route("/api/upload", post(upload_handler))
        .route("/api/clear", post(clear_memory_handler))
        // 靜態資源服務（相當於 app.mount("/static", StaticFiles(...))）
        .nest_service("/static", ServeDir::new("static"))
        .nest_service(
            "/outputs",
            ServeDir::new(format!("{}/outputs", workspace_path)),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;
    info!("Starting Tiny Agent RS server on http://localhost:8000");

    // `axum::serve` 相當於 uvicorn.run(app, ...)
    axum::serve(listener, app).await?;

    Ok(())
}

// ============================================================
// 路由處理函式
// ============================================================

/// GET / - 回傳前端首頁
async fn root_handler() -> impl IntoResponse {
    // 在 Rust 中，我們手動讀取並回傳 HTML 檔案
    match tokio::fs::read_to_string("static/index.html").await {
        Ok(html) => axum::response::Html(html).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "index.html not found").into_response(),
    }
}

/// POST /api/chat - 串流對話介面（SSE）
// -----------------------------------------------------------
// Rust 學習筆記：SSE（Server-Sent Events）實作
// -----------------------------------------------------------
// Python (FastAPI):
//   return StreamingResponse(sse_generator(), media_type="text/event-stream")
//
// Rust (axum):
//   Sse::new(stream)
//
// 步驟：
//   1. 建立 mpsc channel（tx 發送端 / rx 接收端）
//   2. 用 tokio::spawn 在背景執行 agent.chat_stream(tx)
//   3. 把 rx 包裝成 SSE Stream 回傳給前端
//
// `Sse<impl Stream<Item = Result<Event, Infallible>>>` 是 axum 的 SSE 回應型別。
// -----------------------------------------------------------
async fn chat_handler(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    // 建立 channel：buffer 大小為 100
    let (tx, rx) = mpsc::channel::<Value>(100);

    // 複製 Arc 引用（不複製資料，只增加引用計數）
    let agent_arc = Arc::clone(&state.agent);
    let message = req.message;

    // 在背景 task 中執行 agent
    // `tokio::spawn` 相當於 asyncio.create_task 或 asyncio.ensure_future
    tokio::spawn(async move {
        let mut agent = agent_arc.lock().await;
        agent.chat_stream(message, tx).await;
    });

    // 把 receiver 包裝成 SSE Stream
    // ReceiverStream 把 mpsc::Receiver 轉換成標準的 Stream trait
    let stream = ReceiverStream::new(rx).map(|event| {
        let data = serde_json::to_string(&event).unwrap_or_default();
        // SSE 事件格式：data: <json>\n\n
        Ok::<Event, Infallible>(Event::default().data(data))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// GET /api/status - 取得技能和工具清單
async fn status_handler(State(state): State<AppState>) -> Json<Value> {
    let mut agent = state.agent.lock().await;
    agent.reload_skills().await;
    Json(json!({
        "skills": agent.get_skills_summary(),
        "tools": agent.get_tools_summary()
    }))
}

/// GET /api/memory - 取得當前記憶狀態
async fn memory_handler(State(state): State<AppState>) -> Json<Value> {
    let agent = state.agent.lock().await;
    let messages = agent.memory.get_messages(20);
    let long_term = agent.memory.get_long_term_memory().await;

    Json(json!({
        "stats": {
            "total_messages_in_window": messages.len(),
            "has_long_term_memory": !long_term.is_empty()
        },
        "long_term_memory": long_term
    }))
}

/// GET /api/history - 取得完整歷史和 Token 統計
async fn history_handler(State(state): State<AppState>) -> Json<Value> {
    let agent = state.agent.lock().await;
    Json(json!({
        "messages": agent.get_messages(),
        "tokens": agent.get_tokens()
    }))
}

/// GET /api/outputs - 列出輸出目錄的所有檔案
async fn list_outputs_handler(State(_state): State<AppState>) -> Json<Value> {
    let outputs_path = "./workspace/outputs";
    let mut files = vec![];

    if let Ok(mut dir) = tokio::fs::read_dir(outputs_path).await {
        while let Ok(Some(entry)) = dir.next_entry().await {
            let path = entry.path();
            if path.is_file() {
                if let Ok(meta) = tokio::fs::metadata(&path).await {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        // 取得修改時間
                        let mtime = meta
                            .modified()
                            .ok()
                            .and_then(|t| {
                                t.duration_since(std::time::UNIX_EPOCH).ok()
                            })
                            .map(|d| d.as_secs_f64())
                            .unwrap_or(0.0);

                        files.push(json!({
                            "name": name,
                            "size": meta.len(),
                            "mtime": mtime
                        }));
                    }
                }
            }
        }
    }

    // 按修改時間倒序排列
    files.sort_by(|a, b| {
        let ma = a.get("mtime").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let mb = b.get("mtime").and_then(|v| v.as_f64()).unwrap_or(0.0);
        mb.partial_cmp(&ma).unwrap_or(std::cmp::Ordering::Equal)
    });

    Json(json!({ "files": files }))
}

/// DELETE /api/outputs/:filename - 刪除指定輸出檔案
// -----------------------------------------------------------
// Rust 學習筆記：路徑參數（Path Parameter）
// -----------------------------------------------------------
// Python (FastAPI):  @app.delete("/api/outputs/{filename}")
//                    async def delete(filename: str): ...
//
// Rust (axum):       async fn delete_output_handler(
//                        AxumPath(filename): AxumPath<String>
//                    ) -> ... { ... }
// -----------------------------------------------------------
async fn delete_output_handler(
    AxumPath(filename): AxumPath<String>,
) -> Json<Value> {
    // 安全性：防止路徑穿越攻擊（directory traversal）
    if filename.contains("..") || filename.contains('/') || filename.contains('\\') {
        return Json(json!({"status": "error", "message": "Invalid filename"}));
    }

    let file_path = format!("./workspace/outputs/{}", filename);

    match tokio::fs::remove_file(&file_path).await {
        Ok(_) => Json(json!({"status": "success", "message": format!("Deleted {}", filename)})),
        Err(e) => Json(json!({"status": "error", "message": e.to_string()})),
    }
}

/// POST /api/upload - 上傳檔案到工作區
// -----------------------------------------------------------
// Rust 學習筆記：multipart 表單處理
// -----------------------------------------------------------
// Python (FastAPI):  async def upload(file: UploadFile = File(...)): ...
//
// Rust (axum):       async fn upload_handler(mut multipart: Multipart) -> ... {
//                        while let Some(field) = multipart.next_field().await? { ... }
//                    }
// -----------------------------------------------------------
async fn upload_handler(mut multipart: Multipart) -> Json<Value> {
    let outputs_path = "./workspace/outputs";
    let _ = tokio::fs::create_dir_all(outputs_path).await;

    while let Ok(Some(field)) = multipart.next_field().await {
        let filename = match field.file_name() {
            Some(name) => name.to_string(),
            None => continue,
        };

        let data = match field.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return Json(json!({"status": "error", "message": e.to_string()}));
            }
        };

        let file_path = format!("{}/{}", outputs_path, filename);
        match tokio::fs::write(&file_path, &data).await {
            Ok(_) => return Json(json!({"status": "success", "filename": filename})),
            Err(e) => return Json(json!({"status": "error", "message": e.to_string()})),
        }
    }

    Json(json!({"status": "error", "message": "No file uploaded"}))
}

/// POST /api/clear - 清除記憶會話
async fn clear_memory_handler(State(state): State<AppState>) -> Json<Value> {
    let mut agent = state.agent.lock().await;
    agent.clear_memory().await;
    Json(json!({"status": "ok"}))
}
