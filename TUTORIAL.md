# tiny_agent_rs 完全教學（繁體中文）

> 本教學假設你熟悉 Python，但對 Rust 幾乎不熟悉。
> 每個概念都會對比 Python 版的寫法來解釋。

---

## 目錄

1. [為什麼要轉到 Rust？](#1-為什麼要轉到-rust)
2. [Rust 最重要的觀念](#2-rust-最重要的觀念)
3. [專案結構說明](#3-專案結構說明)
4. [Cargo.toml — Rust 的 requirements.txt](#4-cargotoml--rust-的-requirementstxt)
5. [memory.rs — 記憶存儲模組](#5-memoryrs--記憶存儲模組)
6. [tools.rs — 工具定義模組](#6-toolsrs--工具定義模組)
7. [skills.rs — 技能載入模組](#7-skillsrs--技能載入模組)
8. [context.rs — 上下文構建模組](#8-contextrs--上下文構建模組)
9. [loop_runner.rs — Agent 執行迴圈](#9-loop_runnerrs--agent-執行迴圈)
10. [agent.rs — TinyAgent 主類別](#10-agentrs--tinyagent-主類別)
11. [main.rs — Web 伺服器入口](#11-mainrs--web-伺服器入口)
12. [如何執行](#12-如何執行)
13. [Rust vs Python 語法對照表](#13-rust-vs-python-語法對照表)

---

## 1. 為什麼要轉到 Rust？

| 比較項目       | Python                  | Rust                          |
|----------------|-------------------------|-------------------------------|
| 執行速度       | 慢（直譯器）            | 極快（編譯後接近 C）          |
| 記憶體用量     | 高（GC 開銷）           | 低（零成本抽象）              |
| 並發安全       | 靠 GIL 避免競爭         | 編譯器保證無競爭條件          |
| 錯誤處理       | 執行時拋出 Exception    | 編譯期強制處理 Result/Option  |
| 部署           | 需要 Python 環境        | 單一靜態二進位檔              |

Rust 的代價是學習曲線較陡。但一旦程式編譯通過，許多執行時錯誤就已被排除。

---

## 2. Rust 最重要的觀念

在看程式碼之前，先理解這三個觀念：

### 2.1 所有權（Ownership）

Rust 中每個值只有**一個擁有者**。當擁有者離開作用域，值就被釋放。

```rust
// Python:
x = "hello"
y = x        # y 和 x 都可以使用
print(x)     # OK

// Rust:
let x = String::from("hello");
let y = x;          // x 的所有權移動給 y（Move）
println!("{}", x);  // ❌ 編譯錯誤！x 已被移走
println!("{}", y);  // ✅ OK
```

### 2.2 借用（Borrowing）

如果只是想「讀取」而不是「佔有」，就用 `&` 借用：

```rust
let x = String::from("hello");
let y = &x;          // y 借用 x（不移動所有權）
println!("{}", x);   // ✅ OK，x 仍然是擁有者
println!("{}", y);   // ✅ OK，y 是參考
```

### 2.3 Result 和 Option

Rust 沒有 `None`/`null` 和 `Exception`。改用：

```python
# Python
def find_user(id) -> User | None:
    ...

def read_file(path) -> str:    # 可能拋出 FileNotFoundError
    ...
```

```rust
// Rust
fn find_user(id: u64) -> Option<User> { ... }  // Some(user) 或 None

fn read_file(path: &str) -> Result<String, io::Error> { ... }  // Ok(內容) 或 Err(錯誤)
```

`?` 運算子等同於 Python 的「如果是 None/錯誤就立刻 return」：

```rust
fn process() -> Result<(), Error> {
    let content = read_file("foo.txt")?;  // 如果讀取失敗，立刻 return Err
    println!("{}", content);
    Ok(())
}
```

---

## 3. 專案結構說明

```
tiny_agent_rs/
├── Cargo.toml          ← 相當於 requirements.txt + setup.py
├── config.yaml         ← 設定檔（和 Python 版相同格式）
├── static/
│   └── index.html      ← 前端網頁（直接從 Python 版複製）
├── workspace/
│   └── outputs/        ← Agent 的輸出目錄
└── src/
    ├── main.rs         ← 程式入口 + Web 伺服器（對應 app.py）
    ├── agent.rs        ← TinyAgent 主類別（對應 core/agent.py）
    ├── memory.rs       ← 記憶存儲（對應 core/memory.py）
    ├── loop_runner.rs  ← Agent 迴圈（對應 core/loop.py）
    ├── context.rs      ← 上下文構建（對應 core/context.py）
    ├── tools.rs        ← 工具定義（對應 core/tools.py）
    └── skills.rs       ← 技能載入（對應 core/skills.py）
```

> **為什麼 loop.py → loop_runner.rs？**
> 因為 `loop` 是 Rust 的保留關鍵字（用於迴圈），不能當作模組名稱。

---

## 4. Cargo.toml — Rust 的 requirements.txt

```toml
# Python 的 requirements.txt 只有套件名稱和版本。
# Rust 的 Cargo.toml 更像 pyproject.toml，還包含了專案元數據。

[package]
name = "tiny_agent_rs"
version = "0.1.0"
edition = "2021"        ← Rust 語言版本（不是編譯器版本）

[dependencies]
tokio = { version = "1", features = ["full"] }  ← 非同步執行時（asyncio 的對應）
axum = "0.7"                                     ← Web 框架（FastAPI 的對應）
async-openai = "0.27"                            ← OpenAI 客戶端
serde = { version = "1", features = ["derive"] } ← 序列化框架（pydantic 的對應）
serde_json = "1"                                 ← JSON 處理（json 模組的對應）
anyhow = "1"                                     ← 錯誤處理輔助
```

**安裝套件**：不需要手動 `pip install`，執行 `cargo build` 時會自動下載。

---

## 5. memory.rs — 記憶存儲模組

### Python 版（core/memory.py）

```python
class MemoryStore:
    def __init__(self, workspace_dir: str, session_id: str = "default"):
        self.memory_dir = os.path.join(workspace_dir, "memory")
        os.makedirs(self.memory_dir, exist_ok=True)
        self.messages: List[Dict[str, Any]] = self._load_history()

    def add_message(self, message: Dict[str, Any]):
        self.messages.append(message)
        self._save_history()
```

### Rust 版（src/memory.rs）

```rust
pub struct MemoryStore {
    pub messages: Vec<Value>,   // Vec<Value> ≈ list[dict]
    tokens: Tokens,
    history_file: PathBuf,      // PathBuf ≈ pathlib.Path
    // ...
}

impl MemoryStore {
    pub async fn new(workspace_dir: &str, session_id: &str) -> Result<Self> {
        let memory_dir = PathBuf::from(workspace_dir).join("memory");
        fs::create_dir_all(&memory_dir).await?;   // ? = 錯誤時立刻 return
        // ...
        Ok(Self { messages, tokens, history_file, ... })
    }

    pub async fn add_message(&mut self, msg: Value) {
        self.messages.push(msg);         // push ≈ list.append
        let _ = self.save_history().await;
    }
}
```

### 關鍵差異

| Python                      | Rust                                    | 說明                            |
|-----------------------------|-----------------------------------------|---------------------------------|
| `self.messages.append(msg)` | `self.messages.push(msg)`               | 加入元素的方法名稱不同          |
| `def __init__(self, ...)`   | `pub async fn new(...) -> Result<Self>` | Rust 沒有 __init__，用 new 慣例 |
| `Dict[str, Any]`            | `serde_json::Value`                     | JSON 動態型別                   |
| `List[...]`                 | `Vec<...>`                              | 動態陣列                        |
| `pathlib.Path`              | `std::path::PathBuf`                    | 路徑類型                        |

---

## 6. tools.rs — 工具定義模組

### Python 的抽象基底類別

```python
class BaseTool:
    def execute(self, **kwargs) -> str:
        raise NotImplementedError("子類別必須實作 execute 方法")

class ReadFileTool(BaseTool):
    async def execute(self, path: str) -> str:
        with open(path) as f:
            return f.read()
```

### Rust 的 trait（特徵）

```rust
// trait 定義了「必須實作的方法」，相當於 Python 的 ABC
#[async_trait]  // 讓 trait 可以使用 async fn
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    async fn execute(&self, args: Value) -> String;
    
    // trait 可以有預設實作（Python 的 mixin）
    fn to_openai_function(&self) -> Value {
        json!({ "name": self.name(), ... })
    }
}

// 實作 trait：等同於 Python 的繼承
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str { "read_file" }
    
    async fn execute(&self, args: Value) -> String {
        let path = args.get("path").and_then(|p| p.as_str())
            .unwrap_or_default();
        fs::read_to_string(path).await.unwrap_or_default()
    }
}
```

### 工具注冊中心（ToolRegistry）

Python 的多型（polymorphism）是隱式的：

```python
tools = {"read_file": ReadFileTool(), "exec": ShellTool()}
```

Rust 需要明確使用「動態分派（dynamic dispatch）」：

```rust
// Box<dyn Tool> = 「在堆積上的、實作了 Tool trait 的某個型別」
// 因為 ReadFileTool 和 ShellTool 大小不同，需要用 Box（指標）包裝
tools: HashMap<String, Box<dyn Tool>>
```

---

## 7. skills.rs — 技能載入模組

這個模組負責掃描 `workspace/skills/` 目錄下的 SKILL.md 檔案。

### 重要 Rust 概念：String vs &str

```rust
// &str = 字串切片（借用），不擁有資料，通常用於函式參數
// String = 擁有的字串，存在 heap 上，可以修改

// 函式參數接收 &str（借用），不需要複製字串
pub fn new(workspace: &str) -> Self { ... }

// struct 欄位用 String（擁有），因為 struct 需要管理自己的資料
pub struct Skill {
    pub name: String,      // ✅ 擁有字串
    // name: &str,         // ❌ 需要 lifetime 標注，複雜化
}
```

### WalkDir（替代 os.walk）

```python
# Python
for root, dirs, files in os.walk(skills_dir):
    for file in files:
        if file == "SKILL.md":
            process(os.path.join(root, file))
```

```rust
// Rust（使用 walkdir crate）
let entries: Vec<_> = WalkDir::new(&self.skills_dir)
    .into_iter()
    .filter_map(|e| e.ok())          // 忽略讀取錯誤
    .filter(|e| e.file_name() == "SKILL.md")
    .collect();                       // 收集成 Vec

for entry in entries {
    let path = entry.path();
    // ...
}
```

---

## 8. context.rs — 上下文構建模組

這個模組負責組裝發給 LLM 的完整訊息 Payload。

### 多行字串（Multiline String）

```python
# Python
text = f"""你名叫 tinybot。
## 當前時間
{now}
"""
```

```rust
// Rust：使用 format! 巨集和 r#"..."# 原始字串
let text = format!(
    r#"你名叫 tinybot。
## 當前時間
{now}
"#
);
// r#"..."# 裡面可以包含 " 和 \ 不需要跳脫
```

### 字串連接

```python
# Python
parts = ["section 1", "section 2"]
result = "\n\n---\n\n".join(parts)
```

```rust
// Rust：幾乎一樣！
let parts = vec!["section 1".to_string(), "section 2".to_string()];
let result = parts.join("\n\n---\n\n");
```

---

## 9. loop_runner.rs — Agent 執行迴圈

這是最複雜的模組，負責與 LLM API 串流互動並執行工具。

### 9.1 async fn 和 .await

```python
# Python
async def run(self, messages):
    response = await client.chat.completions.create(...)
    async for chunk in response:
        yield {"type": "text_delta", "content": chunk.choices[0].delta.content}
```

```rust
// Rust
pub async fn run(&self, messages: Vec<Value>, tx: &EventSendr) {
    let stream = client.chat().create_stream(request).await?;
    while let Some(chunk) = stream.next().await {
        tx.send(json!({"type": "text_delta", "content": ...})).await;
    }
}
```

### 9.2 AsyncGenerator → Channel

Python 可以用 `yield` 在函式中途返回值。Rust 沒有 `yield`（stable 版本）。

最常用的替代方案是 `tokio::sync::mpsc` channel（管道）：

```
Python Generator:                  Rust Channel:

async def chat_stream():           async fn chat_stream(tx: Sender) {
    yield event1                       tx.send(event1).await;
    yield event2          ────>        tx.send(event2).await;
    yield event3                       tx.send(event3).await;
                                   }

for event in chat_stream():        let (tx, rx) = mpsc::channel(100);
    process(event)                 tokio::spawn(async move { chat_stream(tx).await; });
                                   while let Some(event) = rx.recv().await {
                                       process(event);
                                   }
```

圖示：

```
[agent.chat_stream] ──tx──> [channel buffer] ──rx──> [axum SSE handler]
      發送端                                               接收端
    （生產者）                                           （消費者）
```

### 9.3 串流 Tool Call 的累積

LLM 的 tool call 資訊會被分割成多個片段串流過來：

```
片段 1: { id: "call_abc", function: { name: "read" } }
片段 2: { function: { arguments: '{"pa' } }
片段 3: { function: { arguments: 'th":' } }
片段 4: { function: { arguments: '"foo.txt"}' } }
```

我們需要用 `HashMap` 把片段按 index 累積起來：

```rust
// Python
tool_call_buffer = {}  # {index: {id, name, arguments}}
for tc in delta.tool_calls:
    if tc.index not in tool_call_buffer:
        tool_call_buffer[tc.index] = {"id": "", "name": "", "arguments": ""}
    tool_call_buffer[tc.index]["arguments"] += tc.function.arguments

// Rust
let mut tool_call_buffer: HashMap<u32, ToolCallAccumulator> = HashMap::new();
for tc in tool_calls {
    let acc = tool_call_buffer
        .entry(tc.index)
        .or_insert_with(|| ToolCallAccumulator { id: String::new(), ... });
    if let Some(args) = &tc.function.arguments {
        acc.arguments.push_str(args);  // push_str ≈ str +=
    }
}
```

---

## 10. agent.rs — TinyAgent 主類別

### Arc<Mutex<T>> — Rust 的執行緒安全共享狀態

```python
# Python：有 GIL 保護，直接把 agent 放在模組頂層即可
agent = TinyAgent(...)

@app.post("/api/chat")
async def chat(req):
    async for event in agent.chat_stream(req.message):
        yield event
```

```rust
// Rust：需要明確聲明跨執行緒共享
// Arc = Atomic Reference Count（原子引用計數）
// Mutex = 互斥鎖（同一時間只有一個 task 可以存取）
let agent = Arc::new(Mutex::new(TinyAgent::new(...).await?));

// 在 axum handler 中：
async fn chat_handler(State(state): State<AppState>, ...) {
    let mut agent = state.agent.lock().await;  // 取得鎖（lock）
    agent.chat_stream(message, tx).await;
    // 函式結束時，lock 自動釋放（RAII）
}
```

### 為什麼需要 Arc<Mutex<T>>？

```
請求 1 → handler 1 ─┐
                     ├─ 兩個 handler 同時想修改 agent → 競爭條件！
請求 2 → handler 2 ─┘

解決方案（Mutex）：
請求 1 → handler 1 → lock() → 修改 agent → 釋放鎖
請求 2 → handler 2 →           等待鎖   → lock() → 修改 agent → 釋放鎖
```

---

## 11. main.rs — Web 伺服器入口

### 路由定義

```python
# FastAPI（裝飾器語法）
@app.get("/")
async def root(): ...

@app.post("/api/chat")
async def chat(req: ChatRequest): ...
```

```rust
// axum（函式指標語法）
let app = Router::new()
    .route("/", get(root_handler))
    .route("/api/chat", post(chat_handler))
    .with_state(state);
```

### SSE（Server-Sent Events）回應

```python
# Python (FastAPI)
return StreamingResponse(sse_generator(), media_type="text/event-stream")
```

```rust
// Rust (axum)
async fn chat_handler(...) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::channel(100);
    
    tokio::spawn(async move {
        agent.chat_stream(message, tx).await;  // 背景執行
    });
    
    let stream = ReceiverStream::new(rx).map(|event| {
        Ok(Event::default().data(event.to_string()))
    });
    
    Sse::new(stream)
}
```

---

## 12. 如何執行

### 第一步：安裝 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 第二步：設定 API Key

編輯 `config.yaml`：

```yaml
llm:
  api_key: "sk-your-api-key-here"
  model: "gpt-4o-mini"
  # base_url: "https://api.deepseek.com/v1"  # 如果用其他服務
```

或使用環境變數：

```bash
export OPENAI_API_KEY="sk-your-api-key-here"
```

### 第三步：編譯並執行

```bash
cd tiny_agent_rs

# 第一次編譯（需要下載依賴，可能需要幾分鐘）
cargo build

# 執行（開發模式）
cargo run

# 執行（正式模式，會優化程式碼）
cargo build --release
./target/release/tiny_agent_rs
```

### 第四步：開啟瀏覽器

前往 `http://localhost:8000`

---

## 13. Rust vs Python 語法對照表

### 基本型別

| Python  | Rust                       | 說明                        |
|---------|----------------------------|-----------------------------|
| `str`   | `&str` / `String`          | 借用/擁有的字串             |
| `int`   | `i32`, `i64`, `u32`, `u64` | 整數（有無正負號 × 位元數） |
| `float` | `f32`, `f64`               | 浮點數                      |
| `bool`  | `bool`                     | 布林值                      |
| `None`  | `Option::None`             | 無值                        |
| `list`  | `Vec<T>`                   | 動態陣列                    |
| `dict`  | `HashMap<K, V>`            | 雜湊表                      |
| `tuple` | `(T1, T2)`                 | 元組                        |

### 控制流程

| Python                | Rust                 |
|-----------------------|----------------------|
| `if x:`               | `if x {`             |
| `elif x:`             | `else if x {`        |
| `for i in range(10):` | `for i in 0..10 {`   |
| `for item in list:`   | `for item in &vec {` |
| `while True:`         | `loop {`             |
| `break`               | `break`              |
| `continue`            | `continue`           |

### 函式和類別

| Python                     | Rust                                      |
|----------------------------|-------------------------------------------|
| `def func(x: int) -> str:` | `fn func(x: i32) -> String {`             |
| `async def func():`        | `async fn func() {`                       |
| `class Foo:`               | `struct Foo { ... }` + `impl Foo { ... }` |
| `class Foo(Bar):`          | `impl Bar for Foo { ... }`                |
| `super().__init__()`       | 不適用（Rust 沒有繼承）                   |

### 錯誤處理

| Python                            | Rust                                           |
|-----------------------------------|------------------------------------------------|
| `try: ... except Exception as e:` | `match result { Ok(v) => ..., Err(e) => ... }` |
| `raise ValueError("msg")`         | `return Err(anyhow!("msg"))`                   |
| `x = func() or "default"`         | `x = func().unwrap_or("default")`              |
| `x = d.get("key")`                | `x = map.get("key")` → `Option<&V>`            |

### 非同步

| Python                        | Rust                                         |
|-------------------------------|----------------------------------------------|
| `await coroutine()`           | `coroutine().await`                          |
| `asyncio.gather(*tasks)`      | `tokio::join!()` 或 `futures::join_all()`    |
| `asyncio.create_task(coro())` | `tokio::spawn(async move { coro().await; })` |
| `async for x in gen:`         | `while let Some(x) = stream.next().await {`  |
| `yield x`                     | `tx.send(x).await` (channel 模式)            |

---

## 補充：常見 Rust 錯誤和解法

### 錯誤：cannot move out of ... because it is borrowed

```rust
let s = String::from("hello");
let r = &s;    // 借用 s
drop(s);       // ❌ 錯誤！s 還被 r 借用中
println!("{}", r);
```

解法：確保借用結束後再移動所有權，或用 `.clone()` 複製一份：

```rust
let s = String::from("hello");
let s2 = s.clone();  // ✅ 複製，兩個都可以用
```

### 錯誤：the trait `Send` is not implemented for...

```rust
// 某些型別不能在多個執行緒之間傳送
tokio::spawn(async move {
    use_non_send_type().await;  // ❌
});
```

解法：用 `Arc<Mutex<T>>` 包裝：

```rust
let data = Arc::new(Mutex::new(my_data));
let data_clone = Arc::clone(&data);
tokio::spawn(async move {
    let mut d = data_clone.lock().await;
    // 現在可以安全使用
});
```

### 錯誤：expected `&str`, found `String`

```rust
fn greet(name: &str) { ... }

let name = String::from("Alice");
greet(name);   // ❌ 型別不符
```

解法：用 `&` 借用，或 `&name` 轉成 `&str`：

```rust
greet(&name);  // ✅ String 自動 deref 成 &str
```

---

## 小結

這個 Rust 版本和 Python 版本在邏輯上完全相同，但：

1. **記憶體安全**：編譯器在編譯期就防止了大量執行時錯誤
2. **並發安全**：`Arc<Mutex<T>>` 明確標注了哪些資料需要保護
3. **效能更好**：沒有 GC，沒有直譯器開銷
4. **部署更簡單**：`cargo build --release` 產生單一靜態二進位，不需要 Python 環境

最難適應的是「所有權」和「借用」概念。但這些規則的目的是在編譯期就找出你 Python 程式碼裡可能有的記憶體問題。堅持下去，編譯器是你的朋友！
