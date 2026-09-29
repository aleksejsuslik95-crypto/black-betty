use crate::bridge::{CodeSheet, SheetKind, WorkbenchBlock};

pub struct WorkbenchParser;

impl WorkbenchParser {
    fn determine_kind(path_tag: &str, raw_content: &str) -> (String, SheetKind) {
        if path_tag == "snippet" {
            return (path_tag.to_string(), SheetKind::Snippet);
        }
        if path_tag.ends_with(":full") {
            let clean_path = path_tag.trim_end_matches(":full").to_string();
            return (clean_path, SheetKind::FullFile);
        }
        let has_target_sep = raw_content.lines().any(|l| l.trim() == "---");
        let has_patch_sep = raw_content.lines().any(|l| l.trim() == "+++");
        if has_target_sep && has_patch_sep {
            let mut target_buf = String::new();
            let mut replace_buf = String::new();
            let mut in_target = false;
            let mut in_replace = false;
            for line in raw_content.lines() {
                let trimmed = line.trim();
                if trimmed == "---" {
                    in_target = true;
                    in_replace = false;
                    continue;
                }
                if trimmed == "+++" {
                    in_target = false;
                    in_replace = true;
                    continue;
                }
                if in_target {
                    target_buf.push_str(line);
                    target_buf.push('\n');
                } else if in_replace {
                    replace_buf.push_str(line);
                    replace_buf.push('\n');
                }
            }
            return (
                path_tag.to_string(),
                SheetKind::SearchReplace {
                    target: target_buf.trim_end_matches('\n').to_string(),
                    replacement: replace_buf.trim_end_matches('\n').to_string(),
                },
            );
        }
        (path_tag.to_string(), SheetKind::FullFile)
    }

    pub fn parse_stream(raw_text: &str) -> Vec<WorkbenchBlock> {
        let mut blocks = Vec::new();
        let mut text_buffer = String::new();
        let mut code_buffer = String::new();
        let mut inside_code = false;
        let mut current_sheet_title = String::new();

        for line in raw_text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<<<") && !trimmed.ends_with("<<<END>>>") && trimmed != "<<<END>>>" {
                let clean_tag = trimmed
                    .trim_matches(|c| c == '<' || c == '>' || c == ' ' || c == '`')
                    .trim();
                if !text_buffer.is_empty() {
                    blocks.push(WorkbenchBlock::TextBlock(text_buffer.clone()));
                    text_buffer.clear();
                }
                inside_code = true;
                current_sheet_title = if clean_tag.is_empty() {
                    "snippet".to_string()
                } else {
                    clean_tag.to_string()
                };
                continue;
            }

            if trimmed == "<<<END>>>" || trimmed.ends_with("<<<END>>>") {
                if inside_code {
                    let (clean_id, kind) = Self::determine_kind(&current_sheet_title, &code_buffer);
                    blocks.push(WorkbenchBlock::SheetBlock(CodeSheet {
                        id: clean_id,
                        raw_content: code_buffer.clone(),
                        kind,
                    }));
                    code_buffer.clear();
                    current_sheet_title.clear();
                    inside_code = false;
                }
                continue;
            }

            if inside_code {
                code_buffer.push_str(line);
                code_buffer.push('\n');
            } else {
                text_buffer.push_str(line);
                text_buffer.push('\n');
            }
        }

        if inside_code && !code_buffer.is_empty() {
            let (clean_id, kind) = Self::determine_kind(&current_sheet_title, &code_buffer);
            blocks.push(WorkbenchBlock::SheetBlock(CodeSheet {
                id: format!("{} (Незавершенный)", clean_id),
                raw_content: code_buffer,
                kind,
            }));
        } else if !text_buffer.is_empty() {
            blocks.push(WorkbenchBlock::TextBlock(text_buffer));
        }

        blocks
    }
}
