use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use crate::bridge::{FileSliceQuery, FileSliceSnippet};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DialogTurn {
    pub user_msg: String,
    pub raw_reply: String,
    pub backstep_reply: String,
}

pub fn extract_slice(root_path: &Path, query: &FileSliceQuery) -> Option<FileSliceSnippet> {
    let full_path = root_path.join(&query.relative_path);
    let file = File::open(&full_path).ok()?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().filter_map(Result::ok).collect();

    if lines.is_empty() {
        return None;
    }

    let (start_idx, end_idx) = if let Some(pattern) = &query.pattern {
        let pattern_lower = pattern.to_lowercase();
        // Ищем первое вхождение шаблона
        if let Some(matched_line) = lines.iter().position(|l| l.to_lowercase().contains(&pattern_lower)) {
            let start = matched_line.saturating_sub(0);
            let end = (start + query.line_count.max(1)).min(lines.len());
            (start, end)
        } else {
            return None;
        }
    } else if let Some(start_line) = query.start_line {
        let start = start_line.saturating_sub(1).min(lines.len().saturating_sub(1));
        let end = (start + query.line_count.max(1)).min(lines.len());
        (start, end)
    } else {
        // По умолчанию первые N строк
        let end = query.line_count.max(1).min(lines.len());
        (0, end)
    };

    // Форматируем срез с номерами строк для максимальной наглядности модели
    let mut snippet_content = String::new();
    for (i, line) in lines[start_idx..end_idx].iter().enumerate() {
        let current_num = start_idx + i + 1;
        snippet_content.push_str(&format!("{:4} | {}\n", current_num, line));
    }

    Some(FileSliceSnippet {
        relative_path: query.relative_path.clone(),
        start_line: start_idx + 1,
        end_line: end_idx,
        content: snippet_content,
    })
}

impl DialogTurn {
    pub fn new(user_msg: String, raw_reply: String, backstep_reply: String) -> Self {
        Self {
            user_msg,
            raw_reply,
            backstep_reply,
        }
    }
}

pub struct CodeCompressor;

impl CodeCompressor {
    /// Сжатие кода с учетом синтаксиса файла
    pub fn compress(relative_path: &str, raw_content: &str) -> String {
        if relative_path.ends_with(".json") {
            if let Ok(val) = serde_json::from_str::<Value>(raw_content) {
                return val.to_string();
            }
        }

        let is_hash_comment_syntax = relative_path.ends_with(".gd")
            || relative_path.ends_with(".py")
            || relative_path.ends_with(".toml")
            || relative_path.ends_with(".yaml")
            || relative_path.ends_with(".yml");

        let is_slash_comment_syntax = relative_path.ends_with(".rs")
            || relative_path.ends_with(".slint")
            || relative_path.ends_with(".css");

        if !is_hash_comment_syntax && !is_slash_comment_syntax {
            return raw_content
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n");
        }

        let mut compressed = String::new();
        let mut in_block_comment = false;

        for line in raw_content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if is_hash_comment_syntax {
                if trimmed.starts_with('#') {
                    continue;
                }

                let mut end_idx = line.len();
                let mut inside_quotes = false;
                let mut quote_char = b'"';
                let bytes = line.as_bytes();

                for i in 0..bytes.len() {
                    let b = bytes[i];
                    if (b == b'"' || b == b'\'') && (i == 0 || bytes[i - 1] != b'\\') {
                        if !inside_quotes {
                            inside_quotes = true;
                            quote_char = b;
                        } else if quote_char == b {
                            inside_quotes = false;
                        }
                    } else if b == b'#' && !inside_quotes {
                        end_idx = i;
                        break;
                    }
                }

                let clean_line = line[..end_idx].trim_end();
                if !clean_line.trim().is_empty() {
                    compressed.push_str(clean_line);
                    compressed.push('\n');
                }
                continue;
            }

            // Slash comment syntax
            if in_block_comment {
                if let Some(end_idx) = trimmed.find("*/") {
                    in_block_comment = false;
                    let remaining = &trimmed[end_idx + 2..];
                    if !remaining.trim().is_empty() {
                        let indent_len = line.len() - line.trim_start().len();
                        compressed.push_str(&" ".repeat(indent_len));
                        compressed.push_str(remaining.trim());
                        compressed.push('\n');
                    }
                }
                continue;
            }

            if trimmed.starts_with("/*") {
                if !trimmed.contains("*/") {
                    in_block_comment = true;
                }
                continue;
            }

            if trimmed.starts_with("//") {
                continue;
            }

            let mut end_idx = line.len();
            let mut inside_quotes = false;
            let bytes = line.as_bytes();
            let mut i = 0;

            while i < bytes.len() {
                if bytes[i] == b'"' {
                    if i == 0 || bytes[i - 1] != b'\\' {
                        inside_quotes = !inside_quotes;
                    }
                } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    if !inside_quotes {
                        end_idx = i;
                        break;
                    }
                }
                i += 1;
            }

            let clean_line = line[..end_idx].trim_end();
            if !clean_line.trim().is_empty() {
                compressed.push_str(clean_line);
                compressed.push('\n');
            }
        }

        compressed
    }

    pub fn estimate_tokens(text: &str) -> usize {
        (text.chars().count() as f64 / 3.5).ceil() as usize
    }

    pub fn summarize_to_backstep(reply: &str) -> String {
        let mut result = String::new();
        let mut rest = reply;

        while let Some(start_idx) = rest.find("<<<") {
            result.push_str(&rest[..start_idx]);
            let after_start = &rest[start_idx + 3..];

            if let Some(tag_end_rel) = after_start.find(">>>") {
                let tag = after_start[..tag_end_rel].trim();
                let body_start = &after_start[tag_end_rel + 3..];

                if let Some(end_marker_idx) = body_start.find("<<<END>>>") {
                    let code_body = &body_start[..end_marker_idx];
                    let line_count = code_body.lines().count();

                    let summary = if tag == "snippet" {
                        "[BackStep: Сниппет / Команда]".to_string()
                    } else if tag.ends_with(":full") {
                        let path = tag.trim_end_matches(":full");
                        format!("[BackStep: Полный файл '{}' (~{} строк)]", path, line_count)
                    } else {
                        let is_patch = code_body.lines().any(|l| l.trim() == "---") 
                            && code_body.lines().any(|l| l.trim() == "+++");
                        if is_patch {
                            let target_lines = code_body.lines()
                                .skip_while(|l| l.trim() != "---")
                                .take_while(|l| l.trim() != "+++")
                                .count().saturating_sub(1);
                            let repl_lines = code_body.lines()
                                .skip_while(|l| l.trim() != "+++")
                                .count().saturating_sub(1);
                            format!("[BackStep: Патч для '{}' (замена {} -> {} строк)]", tag, target_lines, repl_lines)
                        } else {
                            format!("[BackStep: Правка файла '{}' (~{} строк)]", tag, line_count)
                        }
                    };

                    result.push_str(&format!("\n{}\n", summary));
                    rest = &body_start[end_marker_idx + 9..];
                } else {
                    result.push_str(&rest[start_idx..]);
                    rest = "";
                    break;
                }
            } else {
                result.push_str(&rest[start_idx..]);
                rest = "";
                break;
            }
        }
        result.push_str(rest);

        result
            .lines()
            .map(|l| l.trim_end())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }
}