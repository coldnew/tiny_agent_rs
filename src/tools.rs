// ============================================================
// tools.rs - 工具定義與執行模組
// 對應 Python 版的 core/tools.py
//
// 職責：
//   1. 定義工具的 trait（抽象介面）
//   2. 實作各種工具（讀檔、寫檔、編輯、Shell）
//   3. 工具的統一管理（ToolRegistry）
// ============================================================

use async_trait::async_trait;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::fs;
use tokio::process::Command;

// -----------------------------------------------------------
// Rust 學習筆記：trait（特徵/介面）
// -----------------------------------------------------------
// Python:  class BaseTool:
//              def name(self): raise NotImplementedError
//              def execute(self, **kwargs): raise NotImplementedError
//
// Rust:    trait Tool {
//              fn name(&self) -> &str;
//              async fn execute(&self, args: Value) -> String;
//          }
//
// trait 定義了「必須實作的方法」，相當於 Python 的抽象基底類別。
// 任何 struct 只要 `impl Tool for MyStruct { ... }` 就算實作了這個介面。
//
// `#[async_trait]` 是讓 trait 支援 async fn 的輔助巨集。
// Rust 原生 trait 對 async fn 的支援還在穩定化中，
// 目前最簡單的做法是用 async-trait 這個 crate。
//
// `Send + Sync` 讓這個 trait 物件可以跨執行緒使用（Tokio 的多執行緒需要）。
// -----------------------------------------------------------
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;

    // -----------------------------------------------------------
    // trait 的預設方法（default method）
    // -----------------------------------------------------------
    // 在 trait 裡可以提供預設實作，子類別不用再重複寫。
    // 相當於 Python 的 BaseTool.to_openai_function(self)。
    // -----------------------------------------------------------
    fn to_openai_function(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters()
            }
        })
    }

    // args 是 serde_json::Value，相當於 Python 的 dict
    async fn execute(&self, args: Value) -> String;
}

// ============================================================
// ReadFileTool - 讀取檔案工具
// ============================================================

// -----------------------------------------------------------
// Rust 學習筆記：unit struct（空結構體）
// -----------------------------------------------------------
// Python:  class ReadFileTool(BaseTool): ...
//
// Rust:    pub struct ReadFileTool;
//
// 這個 struct 沒有任何欄位，叫做 unit struct。
// 它的「類別方法」和「行為」都定義在 impl 區塊裡。
// -----------------------------------------------------------
pub struct ReadFileTool;

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "讀取指定檔案的內容。注意，如果檔案太大可能會截斷。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要讀取的檔案的絕對或相對路徑"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> String {
        // -----------------------------------------------------------
        // Rust 學習筆記：Option 的鏈式操作
        // -----------------------------------------------------------
        // Python:  path = args.get("path")
        //          if path is None: return "錯誤"
        //
        // Rust:    args.get("path")          → Option<&Value>
        //              .and_then(|v| v.as_str()) → Option<&str>
        //
        // Option 有兩種狀態：Some(值) 或 None。
        // `and_then` 相當於：如果是 Some 就繼續，None 就停止。
        // -----------------------------------------------------------
        let path = match args.get("path").and_then(|p| p.as_str()) {
            Some(p) => p.to_string(),
            None => return "錯誤：缺少 path 參數".to_string(),
        };

        match fs::read_to_string(&path).await {
            Ok(content) => {
                if content.len() > 10000 {
                    format!("{}\n...[檔案內容過長被截斷]", &content[..10000])
                } else {
                    content
                }
            }
            Err(e) => format!("讀取檔案失敗: {}", e),
        }
    }
}

// ============================================================
// WriteFileTool - 寫入檔案工具
// ============================================================
pub struct WriteFileTool;

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "將內容寫入到指定檔案中。如果檔案不存在則會建立，如果存在則會覆蓋。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要寫入的檔案路徑"
                },
                "content": {
                    "type": "string",
                    "description": "要寫入的內容文字"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> String {
        let path = match args.get("path").and_then(|p| p.as_str()) {
            Some(p) => p.to_string(),
            None => return "錯誤：缺少 path 參數".to_string(),
        };
        let content = match args.get("content").and_then(|c| c.as_str()) {
            Some(c) => c.to_string(),
            None => return "錯誤：缺少 content 參數".to_string(),
        };

        // 確保父目錄存在
        if let Some(parent) = Path::new(&path).parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = fs::create_dir_all(parent).await {
                    return format!("建立目錄失敗: {}", e);
                }
            }
        }

        match fs::write(&path, content).await {
            Ok(_) => format!("成功寫入檔案: {}", path),
            Err(e) => format!("寫入檔案失敗: {}", e),
        }
    }
}

// ============================================================
// EditFileTool - 編輯檔案工具（查找替換）
// ============================================================
pub struct EditFileTool;

#[async_trait]
impl Tool for EditFileTool {
    fn name(&self) -> &str {
        "edit_file"
    }

    fn description(&self) -> &str {
        "編輯指定檔案的內容。通過查找舊字串並替換為新字串。建議先讀取檔案確認內容。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "要編輯的檔案路徑"
                },
                "old_str": {
                    "type": "string",
                    "description": "要被替換的原始文字字串"
                },
                "new_str": {
                    "type": "string",
                    "description": "替換後的新文字字串"
                }
            },
            "required": ["path", "old_str", "new_str"]
        })
    }

    async fn execute(&self, args: Value) -> String {
        let path = match args.get("path").and_then(|p| p.as_str()) {
            Some(p) => p.to_string(),
            None => return "錯誤：缺少 path 參數".to_string(),
        };
        let old_str = match args.get("old_str").and_then(|s| s.as_str()) {
            Some(s) => s.to_string(),
            None => return "錯誤：缺少 old_str 參數".to_string(),
        };
        let new_str = match args.get("new_str").and_then(|s| s.as_str()) {
            Some(s) => s.to_string(),
            None => return "錯誤：缺少 new_str 參數".to_string(),
        };

        let content = match fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => return format!("讀取檔案失敗: {}", e),
        };

        if !content.contains(&old_str) {
            return "錯誤：在檔案內容中未找到指定的 old_str".to_string();
        }

        let new_content = content.replace(&old_str, &new_str);
        match fs::write(&path, new_content).await {
            Ok(_) => format!("成功編輯檔案: {}", path),
            Err(e) => format!("編輯檔案失敗: {}", e),
        }
    }
}

// ============================================================
// ShellTool - Shell 命令執行工具
// ============================================================

// -----------------------------------------------------------
// Rust 學習筆記：帶欄位的 struct
// -----------------------------------------------------------
// Python:  class ShellTool(BaseTool):
//              def __init__(self, timeout=60):
//                  self.timeout = timeout
//
// Rust:    pub struct ShellTool { timeout_secs: u64 }
//          impl ShellTool { pub fn new(timeout_secs: u64) -> Self { ... } }
//
// Rust 沒有 __init__，慣例是提供一個 `new` 關聯函式（靜態方法）。
// -----------------------------------------------------------
pub struct ShellTool {
    timeout_secs: u64,
    deny_patterns: Vec<Regex>,
}

impl ShellTool {
    pub fn new(timeout_secs: u64) -> Self {
        // -----------------------------------------------------------
        // Rust 學習筆記：Vec（動態陣列）
        // -----------------------------------------------------------
        // Python:  deny_patterns = ["pattern1", "pattern2", ...]
        // Rust:    let patterns = vec!["pattern1", "pattern2", ...];
        //
        // `vec![]` 巨集建立一個 Vec。
        // `.into_iter().filter_map(|p| Regex::new(p).ok()).collect()`
        //   等於 Python 的 [re.compile(p) for p in patterns if valid]
        // -----------------------------------------------------------
        let raw_patterns = vec![
            r"\brm\s+-[rf]{1,2}\b",
            r"\bdel\s+/[fq]\b",
            r"\brmdir\s+/s\b",
            r"(?:^|[;&|]\s*)format\b",
            r"\b(mkfs|diskpart)\b",
            r"\bdd\s+if=",
            r">\s*/dev/sd",
            r"\b(shutdown|reboot|poweroff)\b",
            r":\(\)\s*\{.*\};\s*:",
        ];

        let deny_patterns = raw_patterns
            .into_iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        Self {
            timeout_secs,
            deny_patterns,
        }
    }

    fn guard_command(&self, command: &str) -> Option<String> {
        let lower = command.to_lowercase();
        for pattern in &self.deny_patterns {
            if pattern.is_match(&lower) {
                return Some(
                    "錯誤: 命令被安全策略攔截（偵測到危險模式）".to_string(),
                );
            }
        }
        None
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "exec"
    }

    fn description(&self) -> &str {
        "執行 Shell 命令並返回輸出。謹慎使用。"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "要執行的 Shell 命令"
                },
                "working_dir": {
                    "type": "string",
                    "description": "可選的執行目錄"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value) -> String {
        let command = match args.get("command").and_then(|c| c.as_str()) {
            Some(c) => c.to_string(),
            None => return "錯誤：缺少 command 參數".to_string(),
        };

        let working_dir = args
            .get("working_dir")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());

        // 安全檢查
        if let Some(error) = self.guard_command(&command) {
            return error;
        }

        // -----------------------------------------------------------
        // Rust 學習筆記：建立子行程（tokio::process::Command）
        // -----------------------------------------------------------
        // Python:  asyncio.create_subprocess_shell(command, ...)
        // Rust:    tokio::process::Command::new("sh").arg("-c").arg(command)
        //
        // Rust 的 Command 是個 builder pattern：
        //   Command::new("sh")   → 建立命令
        //       .arg("-c")       → 加入參數
        //       .arg(&command)   → 加入要執行的字串
        //       .output()        → 執行並等待結果
        // -----------------------------------------------------------
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&command);

        if let Some(dir) = &working_dir {
            cmd.current_dir(dir);
        }

        // tokio::time::timeout 相當於 asyncio.wait_for
        let result = tokio::time::timeout(
            Duration::from_secs(self.timeout_secs),
            cmd.output(),
        )
        .await;

        match result {
            Err(_) => format!("錯誤：命令執行超時（超過 {} 秒）", self.timeout_secs),
            Ok(Err(e)) => format!("執行命令時發生異常: {}", e),
            Ok(Ok(output)) => {
                let mut parts = vec![];

                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                if !stdout.is_empty() {
                    parts.push(stdout);
                }

                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                if !stderr.trim().is_empty() {
                    parts.push(format!("STDERR:\n{}", stderr));
                }

                if !output.status.success() {
                    parts.push(format!(
                        "\n退出狀態碼: {}",
                        output.status.code().unwrap_or(-1)
                    ));
                }

                let result_str = if parts.is_empty() {
                    "(無輸出)".to_string()
                } else {
                    parts.join("\n")
                };

                if result_str.len() > 10000 {
                    format!(
                        "{}\n... (截斷，剩餘 {} 個字元)",
                        &result_str[..10000],
                        result_str.len() - 10000
                    )
                } else {
                    result_str
                }
            }
        }
    }
}

// ============================================================
// ToolRegistry - 工具注冊中心
// ============================================================

// -----------------------------------------------------------
// Rust 學習筆記：trait 物件（dyn Trait）
// -----------------------------------------------------------
// Python:  self.tools: dict[str, BaseTool] = {}
//
// Rust:    tools: HashMap<String, Box<dyn Tool>>
//
// `Box<dyn Tool>` 表示：
//   - `Box<...>` → 在堆積（heap）上分配的資料，我擁有它
//   - `dyn Tool` → 「實作了 Tool trait 的某個型別」（動態分派）
//
// 因為不同工具（ReadFileTool, WriteFileTool...）大小不同，
// Rust 無法在編譯期確定陣列元素大小，所以用 Box（指標）包裝。
// 這類似 Python 的多型（polymorphism）。
// -----------------------------------------------------------
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        // 預設注冊基礎工具
        registry.register(Box::new(ReadFileTool));
        registry.register(Box::new(WriteFileTool));
        registry.register(Box::new(EditFileTool));
        registry.register(Box::new(ShellTool::new(60)));
        registry
    }

    pub fn register(&mut self, tool: Box<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get_definitions(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|t| t.to_openai_function())
            .collect()
    }

    pub fn get_tool_summaries(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|t| {
                json!({
                    "name": t.name(),
                    "description": t.description()
                })
            })
            .collect()
    }

    pub async fn execute(&self, name: &str, arguments_json: &str) -> String {
        let tool = match self.tools.get(name) {
            Some(t) => t,
            None => return format!("錯誤：未找到名為 '{}' 的工具", name),
        };

        let args: Value = match serde_json::from_str(arguments_json) {
            Ok(v) => v,
            Err(_) => return "錯誤：提供的參數不是有效的 JSON 格式".to_string(),
        };

        tool.execute(args).await
    }
}
