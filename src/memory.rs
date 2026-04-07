// ============================================================
// memory.rs - 記憶存儲模組
// 對應 Python 版的 core/memory.py
//
// 職責：
//   1. 儲存對話歷史（短期記憶）
//   2. 儲存 Token 消耗統計
//   3. 讀取長期記憶（MEMORY.md）
// ============================================================

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tokio::fs;

// -----------------------------------------------------------
// Rust 學習筆記：struct（結構體）
// -----------------------------------------------------------
// Python:  class Tokens:
//              prompt = 0
//              completion = 0
//
// Rust:    struct Tokens { prompt: u64, completion: u64 }
//
// Rust 的 struct 只有資料，沒有方法。方法寫在 `impl` 區塊裡。
// `#[derive(...)]` 是「巨集」(macro)，幫我們自動實作 trait：
//   - Serialize / Deserialize: 讓 serde_json 能讀寫這個型別
//   - Default: 讓我們可以呼叫 Tokens::default()（全部填 0）
//   - Clone: 讓我們可以 .clone() 複製一份
//   - Debug: 讓我們可以用 {:?} 印出來
// -----------------------------------------------------------
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Tokens {
    pub prompt: u64,
    pub completion: u64,
}

// -----------------------------------------------------------
// Rust 學習筆記：所有權（Ownership）與欄位存取
// -----------------------------------------------------------
// Python 的 self.memory_dir 是一個字串。
// Rust 用 PathBuf 來代表路徑，它擁有（own）那個路徑字串。
//
// `pub` 表示這個欄位可以從模組外部存取（相當於 Python 沒有 _ 前綴）。
// 沒有 `pub` 的欄位只有模組內部可以存取。
// -----------------------------------------------------------
pub struct MemoryStore {
    pub messages: Vec<Value>, // Vec<Value> 相當於 Python 的 list[dict]
    tokens: Tokens,
    history_file: PathBuf,
    tokens_file: PathBuf,
    long_term_file: PathBuf,
}

// -----------------------------------------------------------
// Rust 學習筆記：impl 區塊（方法實作）
// -----------------------------------------------------------
// Python 的 class 把資料和方法放在一起。
// Rust 把資料放在 struct，把方法放在 impl 區塊。
//
// `impl MemoryStore { ... }` 就是「幫 MemoryStore 加上方法」。
// -----------------------------------------------------------
impl MemoryStore {
    // -----------------------------------------------------------
    // Rust 學習筆記：async fn 和 Result<T, E>
    // -----------------------------------------------------------
    // Python:  async def new(...) -> MemoryStore:
    //              ...
    //
    // Rust:    pub async fn new(...) -> Result<Self>
    //
    // `Result<Self>` 表示：這個函式可能成功（返回 MemoryStore）
    // 或失敗（返回錯誤）。Python 用 raise Exception，Rust 用 Result。
    //
    // `?` 運算子：如果函式返回 Err，就立刻把錯誤往上傳播（類似 raise）。
    //
    // `Self` 就是 MemoryStore 自己（省略重複寫型別名稱）。
    // -----------------------------------------------------------
    pub async fn new(workspace_dir: &str, session_id: &str) -> Result<Self> {
        let memory_dir = PathBuf::from(workspace_dir).join("memory");
        // `?` 相當於 Python 的：
        //   if error: raise error
        fs::create_dir_all(&memory_dir).await?;

        let history_file = memory_dir.join(format!("{}_history.json", session_id));
        let tokens_file = memory_dir.join(format!("{}_tokens.json", session_id));
        let long_term_file = memory_dir.join("MEMORY.md");

        let messages = Self::load_history(&history_file).await;
        let tokens = Self::load_tokens(&tokens_file).await;

        Ok(Self {
            messages,
            tokens,
            history_file,
            tokens_file,
            long_term_file,
        })
    }

    // -----------------------------------------------------------
    // Rust 學習筆記：私有方法（沒有 pub）
    // -----------------------------------------------------------
    // 這個方法沒有 `pub`，所以只能在這個模組內部呼叫。
    // 相當於 Python 的 _load_history（前置底線慣例）。
    //
    // `&PathBuf` 表示借用（borrow）PathBuf 的參考（reference），
    // 不取得所有權，只讀取它的值。
    // -----------------------------------------------------------
    async fn load_history(path: &PathBuf) -> Vec<Value> {
        match fs::read_to_string(path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => vec![], // 讀取失敗就回傳空陣列
        }
    }

    async fn load_tokens(path: &PathBuf) -> Tokens {
        match fs::read_to_string(path).await {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Tokens::default(), // 讀取失敗就回傳全 0
        }
    }

    async fn save_history(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.messages)?;
        fs::write(&self.history_file, content).await?;
        Ok(())
    }

    async fn save_tokens(&self) -> Result<()> {
        let content = serde_json::to_string_pretty(&self.tokens)?;
        fs::write(&self.tokens_file, content).await?;
        Ok(())
    }

    // -----------------------------------------------------------
    // Rust 學習筆記：&mut self
    // -----------------------------------------------------------
    // Python:  def add_message(self, message): ...
    //
    // Rust 的 self 有三種形式：
    //   self       → 取得所有權（消耗自己）
    //   &self      → 不可變借用（只讀）
    //   &mut self  → 可變借用（可以修改內部狀態）
    //
    // `push` 需要修改 self.messages，所以要用 &mut self。
    // -----------------------------------------------------------
    pub async fn add_message(&mut self, msg: Value) {
        self.messages.push(msg);
        // 忽略儲存失敗（使用 let _ = 丟棄結果）
        let _ = self.save_history().await;
    }

    pub async fn add_tokens(&mut self, prompt: u64, completion: u64) {
        self.tokens.prompt += prompt;
        self.tokens.completion += completion;
        let _ = self.save_tokens().await;
    }

    pub fn get_tokens(&self) -> &Tokens {
        &self.tokens
    }

    // -----------------------------------------------------------
    // Rust 學習筆記：回傳擁有的資料
    // -----------------------------------------------------------
    // 這個方法回傳 Vec<Value>（擁有的資料），所以需要 .clone()。
    // 如果只是讀取，可以回傳 &[Value]（切片參考），但這裡為了簡單就 clone。
    // -----------------------------------------------------------
    pub fn get_messages(&self, window_size: usize) -> Vec<Value> {
        if self.messages.len() <= window_size {
            return self.messages.clone();
        }

        // 安全截斷：找到一個 user 訊息作為起點
        let mut start_idx = self.messages.len().saturating_sub(window_size);
        while start_idx > 0 {
            let role = self.messages[start_idx]
                .get("role")
                .and_then(|r| r.as_str());
            if role == Some("user") {
                break;
            }
            start_idx -= 1;
        }

        self.messages[start_idx..].to_vec()
    }

    pub async fn get_long_term_memory(&self) -> String {
        fs::read_to_string(&self.long_term_file)
            .await
            .unwrap_or_default()
    }

    pub async fn clear_history(&mut self) {
        self.messages.clear();
        self.tokens = Tokens::default();
        let _ = self.save_history().await;
        let _ = self.save_tokens().await;
    }
}
