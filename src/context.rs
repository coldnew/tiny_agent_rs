// ============================================================
// context.rs - 上下文構建模組
// 對應 Python 版的 core/context.py
//
// 職責：
//   1. 組裝系統提示詞（包含人格設定、技能、長期記憶）
//   2. 組裝完整的訊息 Payload 發送給大模型
// ============================================================

use chrono::Local;
use serde_json::{json, Value};

use crate::memory::MemoryStore;
use crate::skills::SkillsLoader;

// -----------------------------------------------------------
// Rust 學習筆記：模組系統和 use 宣告
// -----------------------------------------------------------
// Python:  from .memory import MemoryStore
//          from .skills import SkillsLoader
//
// Rust:    use crate::memory::MemoryStore;
//          use crate::skills::SkillsLoader;
//
// `crate` 代表當前的 crate（套件）根目錄。
// 模組路徑用 :: 分隔（Python 用 .）。
// -----------------------------------------------------------

pub struct ContextBuilder {
    workspace_dir: String,
}

impl ContextBuilder {
    pub fn new(workspace_dir: &str) -> Self {
        Self {
            workspace_dir: workspace_dir.to_string(),
        }
    }

    // -----------------------------------------------------------
    // Rust 學習筆記：借用多個參數
    // -----------------------------------------------------------
    // 這個方法需要讀取 memory 和 skills，但不需要修改它們，
    // 所以用 &MemoryStore 和 &SkillsLoader（不可變借用）。
    //
    // Python 可以隨時傳遞物件，Rust 需要明確聲明借用。
    // -----------------------------------------------------------
    pub async fn build_system_prompt(
        &self,
        memory: &MemoryStore,
        skills: &SkillsLoader,
    ) -> String {
        let mut parts = vec![];

        // 1. 基礎人格與時間設定
        parts.push(self.get_identity());

        // 2. 掛載常駐核心技能
        let always_skills = skills.get_always_skills_prompt();
        if !always_skills.is_empty() {
            parts.push(always_skills);
        }

        // 3. 掛載長期記憶
        let long_term = memory.get_long_term_memory().await;
        if !long_term.is_empty() {
            parts.push(format!("# 工作記憶和參考事實\n\n{}", long_term));
        }

        // 4. 可選技能列表
        let skills_summary = skills.build_skills_summary_prompt();
        if !skills_summary.is_empty() {
            parts.push(skills_summary);
        }

        // -----------------------------------------------------------
        // Rust 學習筆記：join 字串
        // -----------------------------------------------------------
        // Python:  "\n\n---\n\n".join(parts)
        // Rust:    parts.join("\n\n---\n\n")
        //
        // 注意：Rust 的 join 需要 Vec<String>，
        // 如果是 Vec<&str> 也可以，但型別必須一致。
        // -----------------------------------------------------------
        parts.join("\n\n---\n\n")
    }

    fn get_identity(&self) -> String {
        // -----------------------------------------------------------
        // Rust 學習筆記：format! 巨集
        // -----------------------------------------------------------
        // Python:  f"你名叫 tinybot，是一個有用的 AI 助手。\n..."
        // Rust:    format!("你名叫 tinybot，是一個有用的 AI 助手。\n...")
        //
        // `format!` 和 `println!` 很像，但 format! 返回 String。
        // -----------------------------------------------------------
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let tz = Local::now().format("%Z").to_string();
        let workspace = &self.workspace_dir;

        // 取得作業系統資訊
        let os_info = std::env::consts::OS;
        let arch = std::env::consts::ARCH;

        format!(
            r#"你名叫 tinybot，是一個有用的 AI 助手。

## 當前時間
{now} ({tz})

## 運行環境
{os_info} {arch}, Rust

## 工作區
你的工作區位於: {workspace}
- 長期記憶: {workspace}/memory/MEMORY.md
- 歷史日誌: {workspace}/memory/HISTORY.md (支援 grep 搜尋)
- 輸出目錄: {workspace}/outputs/
- 自定義技能: {workspace}/skills/{{skill-name}}/SKILL.md

> [!IMPORTANT]
> **絕對強制要求：** 除了讀取記憶（`memory/`）和讀取技能配置（`skills/`）外，
> 無論是文件、程式碼、圖片、音頻、測試檔案還是任何工具的執行生成產物，
> **只要你需要新建或修改目標檔案存放結果，你《必須》將它們統統存放在 `{workspace}/outputs/` 目錄內**。

直接使用文字回覆對話。僅在需要發送到特定聊天頻道時使用 'message' 工具。

## 工具呼叫指南
- 在呼叫工具之前，你可以簡要說明你的意圖，但絕不要在收到結果之前預測或描述預期的結果。
- 不要假設檔案或目錄存在 — 使用 read_file 或 exec (ls) 來驗證。
- 在使用 edit_file 或 write_file 修改檔案之前，請先閱讀以確認其當前內容。
- 如果工具呼叫失敗，請在嘗試不同方法之前分析錯誤。

## 記憶
- 記住重要的事實：寫入 {workspace}/memory/MEMORY.md
- 回憶過去的事件：使用 grep 搜尋 {workspace}/memory/HISTORY.md"#
        )
    }

    pub async fn build_messages(
        &self,
        current_user_message: &str,
        memory: &MemoryStore,
        skills: &SkillsLoader,
    ) -> Vec<Value> {
        let mut messages = vec![];

        // 第一條永遠是 System Message
        let system_prompt = self.build_system_prompt(memory, skills).await;
        messages.push(json!({
            "role": "system",
            "content": system_prompt
        }));

        // 過去的歷史訊息
        let history = memory.get_messages(20);
        messages.extend(history);

        // 當前用戶輸入
        messages.push(json!({
            "role": "user",
            "content": current_user_message
        }));

        messages
    }
}
