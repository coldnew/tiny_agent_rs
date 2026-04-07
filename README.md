<h1 align="center">TinyAgent RS</h1>

<p align="center">
  <strong>7 個檔案，1 個能跑的 AI Agent — Rust 版</strong><br/>
  完整對應 <a href="../tiny_agent">tiny_agent (Python)</a>，用來學習 Rust 並理解 Agent 本質
</p>

<p align="center">
  <img src="https://img.shields.io/badge/rust-1.75+-orange?style=flat-square&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/framework-axum-blue?style=flat-square" alt="axum" />
  <img src="https://img.shields.io/badge/LLM-OpenAI%20Compatible-412991?style=flat-square&logo=openai" alt="LLM" />
  <img src="https://img.shields.io/badge/async-tokio-green?style=flat-square" alt="tokio" />
</p>

---

## 這是什麼

這是 [tiny_agent（Python 版）](https://github.com/wp931120/tiny_agent.git) 的 Rust 重寫版本。功能完全相同：

- **串流對話** — SSE 即時推送，逐字輸出
- **Tool Calling** — 多輪工具呼叫 + 防死循環保險絲（max 10 輪）
- **對話記憶** — 短期歷史窗口 + 長期 Markdown 記憶
- **技能插件** — 放一個 `SKILL.md` 即裝即用
- **安全防護** — Shell 高危命令正則攔截

如果你想了解 **為什麼每行 Rust 程式碼這樣寫**，請閱讀 [TUTORIAL.md](./TUTORIAL.md)（繁體中文，附大量 Python 對比）。

---

## 專案結構

```
tiny_agent_rs/
├── Cargo.toml          # 套件設定（相當於 requirements.txt）
├── config.yaml         # LLM 設定（API Key、模型、地址）
├── TUTORIAL.md         # Rust 完全教學（繁體中文）
├── static/
│   └── index.html      # 前端網頁
├── workspace/          # 執行時工作區
│   ├── memory/         #   對話記憶存儲
│   ├── skills/         #   技能插件目錄
│   └── outputs/        #   檔案輸出目錄
└── src/
    ├── main.rs         # Web 伺服器入口（對應 app.py）
    ├── agent.rs        # TinyAgent 主類別（對應 core/agent.py）
    ├── loop_runner.rs  # Agent 執行迴圈（對應 core/loop.py）
    ├── tools.rs        # 工具定義與執行（對應 core/tools.py）
    ├── memory.rs       # 記憶存儲模組（對應 core/memory.py）
    ├── skills.rs       # 技能載入模組（對應 core/skills.py）
    └── context.rs      # 上下文構建模組（對應 core/context.py）
```

> `loop.py` → `loop_runner.rs`：因為 `loop` 是 Rust 的保留關鍵字。

---

## 快速開始

### 1. 安裝 Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### 2. 設定 LLM

複製範本並填入你的 API 資訊：

```bash
cp config.yaml.example config.yaml   # 如果你有範本
# 或直接編輯 config.yaml
```

```yaml
llm:
  api_key: "your-api-key-here"
  model: "gpt-4o-mini"
  # base_url: "http://localhost:11434/v1"  # Ollama 或其他相容服務
```

也可以用環境變數：

```bash
export OPENAI_API_KEY="sk-..."
```

### 3. 編譯並執行

```bash
# 開發模式（快速編譯）
cargo run

# 正式模式（優化後的二進位）
cargo build --release
./target/release/tiny_agent_rs
```

前往 `http://localhost:8000` 開始對話。

---

## 核心架構

```
用戶訊息 → ContextBuilder 組裝上下文 → AgentLoop 呼叫 LLM
                                          ↓
                                    模型返回文字？→ 串流輸出給用戶
                                    模型要用工具？→ ToolRegistry 執行
                                          ↓
                                    結果注入上下文 → 再次呼叫 LLM
                                          ↓
                                    循環直到模型說完 → MemoryStore 保存
```

### Python vs Rust 關鍵設計差異

| 概念 | Python 版 | Rust 版 |
|------|-----------|---------|
| 多型工具 | `class BaseTool` 繼承 | `trait Tool` + `Box<dyn Tool>` |
| 串流事件 | `AsyncGenerator` + `yield` | `mpsc::channel` + `tx.send()` |
| 共享狀態 | 模組頂層變數（靠 GIL） | `Arc<Mutex<TinyAgent>>` |
| 錯誤處理 | `try/except` + `raise` | `Result<T, E>` + `?` 運算子 |
| Web 框架 | FastAPI | axum |
| 非同步執行時 | asyncio | tokio |

---

## 內建工具

| 工具 | 說明 |
|------|------|
| `read_file` | 讀取檔案內容（超過 10KB 自動截斷） |
| `write_file` | 寫入／建立檔案 |
| `edit_file` | 查找替換編輯檔案 |
| `exec` | 執行 Shell 命令（帶安全攔截、60 秒超時） |

---

## 技能系統

在 `workspace/skills/` 下建立目錄，放入 `SKILL.md`：

```
workspace/skills/
└── my-skill/
    └── SKILL.md
```

```markdown
---
description: 我的自定義技能描述
active: true
always_load: false
---

# 技能內容

當用戶需要... 請按以下步驟操作...
```

- `always_load: true` → 全文注入系統提示詞（核心技能）
- `always_load: false` → 僅索引名稱和描述，按需讀取（省 token）

---

## API 端點

| 方法 | 路徑 | 說明 |
|------|------|------|
| `POST` | `/api/chat` | 串流對話（SSE） |
| `GET` | `/api/status` | 取得技能和工具清單 |
| `GET` | `/api/memory` | 查看記憶狀態 |
| `GET` | `/api/history` | 取得完整對話歷史 |
| `GET` | `/api/outputs` | 列出輸出檔案 |
| `POST` | `/api/upload` | 上傳檔案到工作區 |
| `POST` | `/api/clear` | 清空對話記憶 |
| `DELETE` | `/api/outputs/:filename` | 刪除輸出檔案 |

---

## 想學 Rust？

閱讀 [TUTORIAL.md](./TUTORIAL.md)，裡面包含：

- Rust 的所有權、借用、生命週期概念（對比 Python）
- 每個模組的詳細解說
- `trait` vs `class`、`Result` vs `Exception`、`channel` vs `yield` 等對照
- 常見 Rust 編譯錯誤和解法
- 完整的 Python ↔ Rust 語法對照表
