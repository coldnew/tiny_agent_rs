// ============================================================
// skills.rs - 技能載入模組
// 對應 Python 版的 core/skills.py
//
// 職責：
//   1. 從工作目錄的 skills/ 資料夾掃描技能
//   2. 解析 SKILL.md 的 YAML frontmatter 和正文
//   3. 提供系統提示詞中的技能摘要
// ============================================================

use regex::Regex;
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::fs;
use walkdir::WalkDir;

// -----------------------------------------------------------
// Rust 學習筆記：有欄位的結構體
// -----------------------------------------------------------
// Python:  class Skill:
//              name: str
//              description: str
//              active: bool
//              always_load: bool
//              path: str
//              content: str
//
// Rust:    struct Skill { name: String, ... }
//
// `String` 是擁有（owned）的字串，儲存在堆積（heap）上。
// `&str`   是借用（borrowed）的字串切片，只是一個參考。
// 在 struct 裡通常用 String（因為 struct 要自己管理記憶體）。
// -----------------------------------------------------------
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub active: bool,
    pub always_load: bool,
    pub path: String,
    pub content: String,
}

pub struct SkillsLoader {
    skills_dir: PathBuf,
    pub skills: Vec<Skill>,
}

impl SkillsLoader {
    pub async fn new(workspace: &str) -> Self {
        let skills_dir = PathBuf::from(workspace).join("skills");

        // 確保 skills 目錄存在
        let _ = fs::create_dir_all(&skills_dir).await;

        let mut loader = Self {
            skills_dir,
            skills: vec![],
        };
        loader.load_all_skills().await;
        loader
    }

    // -----------------------------------------------------------
    // Rust 學習筆記：非同步方法和 &mut self
    // -----------------------------------------------------------
    // 這個方法會修改 self.skills，所以需要 &mut self。
    // `pub async fn` 表示可以被外部呼叫的非同步方法。
    // -----------------------------------------------------------
    pub async fn load_all_skills(&mut self) {
        self.skills.clear();

        // -----------------------------------------------------------
        // Rust 學習筆記：WalkDir 目錄遍歷
        // -----------------------------------------------------------
        // Python:  for root, dirs, files in os.walk(self.skills_dir):
        //              for file in files:
        //                  if file == "SKILL.md": ...
        //
        // Rust:    WalkDir::new(&self.skills_dir)
        //              .into_iter()
        //              .filter_map(|e| e.ok())
        //              .filter(|e| e.file_name() == "SKILL.md")
        //
        // WalkDir 是同步的，所以不需要 .await
        // -----------------------------------------------------------
        let entries: Vec<_> = WalkDir::new(&self.skills_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name() == "SKILL.md")
            .collect();

        for entry in entries {
            let skill_path = entry.path();

            // 技能名稱來自父目錄名稱
            let skill_name = skill_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();

            // 讀取並解析 SKILL.md
            if let Ok(content) = fs::read_to_string(skill_path).await {
                let (meta, body) = parse_frontmatter(&content);

                // -----------------------------------------------------------
                // Rust 學習筆記：從 serde_json::Value 取值
                // -----------------------------------------------------------
                // Python:  meta.get("description", "無描述")
                //
                // Rust:    meta.get("description")
                //              .and_then(|v| v.as_str())
                //              .unwrap_or("無描述")
                //              .to_string()
                //
                // `unwrap_or` 相當於 Python 的 or 預設值。
                // -----------------------------------------------------------
                let description = meta
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("無描述")
                    .to_string();

                let active = meta
                    .get("active")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);

                let always_load = meta
                    .get("always_load")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let path_str = skill_path
                    .to_str()
                    .unwrap_or("")
                    .replace('\\', "/");

                self.skills.push(Skill {
                    name: skill_name,
                    description,
                    active,
                    always_load,
                    path: path_str,
                    content: body,
                });
            }
        }
    }

    pub fn get_always_skills_prompt(&self) -> String {
        // -----------------------------------------------------------
        // Rust 學習筆記：迭代器（Iterator）過濾
        // -----------------------------------------------------------
        // Python:  [s for s in self.skills if s.active and s.always_load]
        //
        // Rust:    self.skills.iter()
        //              .filter(|s| s.active && s.always_load)
        //              .collect::<Vec<_>>()
        // -----------------------------------------------------------
        let always_skills: Vec<&Skill> = self
            .skills
            .iter()
            .filter(|s| s.active && s.always_load)
            .collect();

        if always_skills.is_empty() {
            return String::new();
        }

        let mut parts = vec![
            "# 常駐核心技能 (Always-loaded Skills)".to_string(),
            "你目前具備以下常駐核心技能，你可以隨時使用它們：\n".to_string(),
        ];

        for skill in always_skills {
            parts.push(format!("## 技能：{}", skill.name));
            parts.push(format!("{}\n", skill.content));
        }

        parts.join("\n")
    }

    pub fn build_skills_summary_prompt(&self) -> String {
        let available: Vec<&Skill> = self
            .skills
            .iter()
            .filter(|s| s.active && !s.always_load)
            .collect();

        if available.is_empty() {
            return String::new();
        }

        let mut parts = vec![
            "# 可選擴充技能 (Available Skills)".to_string(),
            "以下技能擴充了你的能力。想使用某項技能前，請務必使用 `read_file` 工具讀取相應路徑下的 SKILL.md 檔案學習具體用法。\n".to_string(),
        ];

        for skill in available {
            parts.push(format!(
                "- **{}**: {}",
                skill.name, skill.description
            ));
            parts.push(format!("  > 技能指南檔案路徑：`{}`", skill.path));
        }

        parts.join("\n")
    }

    pub fn get_skills_summary(&self) -> Vec<Value> {
        self.skills
            .iter()
            .map(|s| {
                json!({
                    "name": s.name,
                    "description": s.description,
                    "active": s.active
                })
            })
            .collect()
    }
}

// ============================================================
// parse_frontmatter - 解析 Markdown frontmatter
// ============================================================

// -----------------------------------------------------------
// Rust 學習筆記：自由函式（free function）
// -----------------------------------------------------------
// 不在 impl 區塊裡的函式叫做自由函式（或模組函式）。
// 它沒有 self，相當於 Python 的模組頂層函式。
//
// 回傳型別 (Value, String) 是個元組（tuple）。
// Python 也有元組，但 Rust 的元組是靜態型別的。
// -----------------------------------------------------------
fn parse_frontmatter(text: &str) -> (Value, String) {
    // 用正則表達式匹配 --- YAML內容 ---
    let re = Regex::new(r"(?s)^---\s*\n(.*?)\n---\s*\n(.*)").unwrap();

    if let Some(captures) = re.captures(text) {
        let yaml_str = captures.get(1).map_or("", |m| m.as_str());
        let body = captures.get(2).map_or("", |m| m.as_str()).trim().to_string();

        // 嘗試解析 YAML
        if let Ok(meta_value) = serde_yaml::from_str::<serde_yaml::Value>(yaml_str) {
            // 將 serde_yaml::Value 轉換成 serde_json::Value
            if let Ok(json_value) = serde_json::to_value(meta_value) {
                return (json_value, body);
            }
        }
    }

    // 解析失敗，返回空 meta 和全文
    (json!({}), text.trim().to_string())
}
