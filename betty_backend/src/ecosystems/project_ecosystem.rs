use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use chrono::Local;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::{broadcast, mpsc};
use crate::bridge::{
    ChatJournalEntry, FocusSelection, ObserverCmd, ProjectCmd,
    SnapshotMeta, TelemetryEnvelope, ToFront,
};

use crate::tools::code_compressor::{extract_slice, CodeCompressor};
use crate::tools::path_finder::PathFinder;

#[derive(Serialize, Deserialize, Clone)]
pub struct ProjectContextState {
    pub project_name: String,
    pub project_root: PathBuf,
    pub file_tree: Vec<String>,
    pub files_hashes: HashMap<String, String>,
    pub compressed_files: HashMap<String, String>,
}

impl ProjectContextState {
    pub fn empty() -> Self {
        Self {
            project_name: "No_Active_Project".to_string(),
            project_root: PathBuf::new(),
            file_tree: Vec::new(),
            files_hashes: HashMap::new(),
            compressed_files: HashMap::new(),
        }
    }

    pub fn load_or_create(path: &Path, name: &str) -> Self {
        let cache_path = path.join(".betty_cache.json");
        if cache_path.exists() {
            if let Ok(data) = fs::read_to_string(&cache_path) {
                if let Ok(mut ctx) = serde_json::from_str::<Self>(&data) {
                    ctx.project_root = path.to_path_buf();
                    ctx.project_name = name.to_string();
                    return ctx;
                }
            }
        }
        let mut ctx = Self::empty();
        ctx.project_name = name.to_string();
        ctx.project_root = path.to_path_buf();
        ctx
    }

    pub fn sync_hashes(&mut self, target_file: Option<&str>) {
        if self.project_name == "No_Active_Project" {
            return;
        }

        let full_project_path = &self.project_root;
        let mut tree_builder = Vec::new();
        let mut content_changed = false;

        fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        let is_ignored = matches!(
                            dir_name,
                            "target" | ".git" | ".godot" | ".betty_history" | "__pycache__"
                                | ".venv" | "venv" | "node_modules" | ".idea" | ".vscode"
                                | "dist" | "build"
                        );
                        if !is_ignored {
                            files.push(path.clone());
                            collect_files(&path, files);
                        }
                    } else {
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            if file_name == ".betty_cache.json"
                                || file_name == ".betty_memory.json"
                                || file_name.ends_with(".bak")
                            {
                                continue;
                            }
                        }
                        if path.extension().map_or(false, |ext| {
                            matches!(
                                ext.to_string_lossy().as_ref(),
                                "rs" | "toml" | "yaml" | "yml" | "txt" | "slint"
                                    | "json" | "md" | "py" | "gd" | "tscn" | "godot" | "css"
                                    | "exe"
                            )
                        }) {
                            files.push(path);
                        }
                    }
                }
            }
        }

        let mut project_files = Vec::new();
        collect_files(full_project_path, &mut project_files);

        for file_path in project_files {
            if let Ok(relative_path_buf) = file_path.strip_prefix(full_project_path) {
                let mut relative_path = relative_path_buf.to_string_lossy().replace('\\', "/");
                if file_path.is_dir() {
                    if !relative_path.ends_with('/') {
                        relative_path.push('/');
                    }
                    tree_builder.push(relative_path);
                    continue;
                }
                tree_builder.push(relative_path.clone());

                if let Some(target) = target_file {
                    if relative_path != target {
                        continue;
                    }
                }

                if let Ok(raw_content) = fs::read_to_string(&file_path) {
                    let mut hasher = Sha256::new();
                    hasher.update(raw_content.as_bytes());
                    let current_hash = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>();
                    let old_hash = self.files_hashes.get(&relative_path);

                    if old_hash.map_or(true, |h| h != &current_hash) {
                        let compressed = CodeCompressor::compress(&relative_path, &raw_content);
                        self.compressed_files.insert(relative_path.clone(), compressed);
                        self.files_hashes.insert(relative_path.clone(), current_hash);
                        content_changed = true;
                    }
                }
            }
        }

        if target_file.is_none() {
            self.file_tree = tree_builder;
            self.files_hashes.retain(|k, _| self.file_tree.contains(k));
            self.compressed_files.retain(|k, _| self.file_tree.contains(k));
        }

        if content_changed {
            let cache_path = full_project_path.join(".betty_cache.json");
            if let Ok(json_data) = serde_json::to_string_pretty(self) {
                let _ = fs::write(cache_path, json_data);
            }
        }
    }

    pub fn build_context(&self, focus: &FocusSelection) -> String {
        let mut code_pool = String::new();
        let mut outline_files = Vec::new();
        let mut hidden_count = 0;

        let normalized_code: HashSet<String> = focus
            .code_files
            .iter()
            .map(|f| f.replace('\\', "/"))
            .collect();

        let normalized_dirs: Vec<String> = focus
            .outline_dirs
            .iter()
            .map(|d| {
                let mut clean = d.replace('\\', "/");
                if !clean.ends_with('/') {
                    clean.push('/');
                }
                clean
            })
            .collect();

        for rel in &self.file_tree {
            if rel.ends_with('/') {
                continue;
            }

            // 1. Файлы с полным сжатым кодом
            if normalized_code.contains(rel) {
                if let Some(comp) = self.compressed_files.get(rel) {
                    code_pool.push_str(&format!("// FILE: {}\n{}\n// END_FILE\n\n", rel, comp));
                } else {
                    code_pool.push_str(&format!("// FILE: {}\n[Код не кэширован]\n// END_FILE\n\n", rel));
                }
                continue;
            }

            // 2. Файлы внутри отмеченных папок (только каркас пути)
            let in_outline_dir = normalized_dirs.iter().any(|dir| rel.starts_with(dir));
            if in_outline_dir {
                outline_files.push(rel.as_str());
            } else {
                hidden_count += 1;
            }
        }

        if !outline_files.is_empty() {
            code_pool.push_str("=== СТРУКТУРА ВЫБРАННЫХ МОДУЛЕЙ (БЕЗ КОДА) ===\n");
            for p in outline_files {
                code_pool.push_str("// [Outline]: ");
                code_pool.push_str(p);
                code_pool.push('\n');
            }
            code_pool.push('\n');
        }

        if hidden_count > 0 {
            code_pool.push_str(&format!(
                "// [Инфо: {} файлов вне фокуса скрыты для экономии контекста]\n\n",
                hidden_count
            ));
        }

        code_pool
    }
}

// ----------------------------------------------------------------------------
// History Manager Helpers
// ----------------------------------------------------------------------------

fn save_snapshot(project_root: &Path, relative_path: &str, reason: &str) -> Result<SnapshotMeta, String> {
    let target = project_root.join(relative_path);
    if !target.is_file() {
        return Err(format!("Файл {} не существует для снапшота", relative_path));
    }
    let content = fs::read(&target).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let hash = hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect::<String>();
    let now = Local::now();
    let snapshot_id = now.format("%Y%m%d_%H%M%S_%3f").to_string();
    let clean_dir_name = relative_path.replace(['/', '\\'], "__");
    let storage_dir = project_root.join(".betty_history").join(clean_dir_name);
    fs::create_dir_all(&storage_dir).map_err(|e| e.to_string())?;

    fs::write(storage_dir.join(format!("{}.bak", snapshot_id)), &content).map_err(|e| e.to_string())?;

    let meta = SnapshotMeta {
        id: snapshot_id.clone(),
        relative_path: relative_path.to_string(),
        timestamp: now.format("%Y-%m-%d %H:%M:%S").to_string(),
        file_size: content.len() as u64,
        hash,
        reason: reason.to_string(),
    };
    if let Ok(json_str) = serde_json::to_string_pretty(&meta) {
        let _ = fs::write(storage_dir.join(format!("{}.json", snapshot_id)), json_str);
    }
    Ok(meta)
}

fn list_snapshots(project_root: &Path, relative_path: &str) -> Vec<SnapshotMeta> {
    let clean_dir_name = relative_path.replace(['/', '\\'], "__");
    let storage_dir = project_root.join(".betty_history").join(clean_dir_name);
    let mut list = Vec::new();
    if let Ok(entries) = fs::read_dir(storage_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().map_or(false, |e| e == "json") {
                if let Ok(data) = fs::read_to_string(p) {
                    if let Ok(meta) = serde_json::from_str::<SnapshotMeta>(&data) {
                        list.push(meta);
                    }
                }
            }
        }
    }
    list.sort_by(|a, b| b.id.cmp(&a.id));
    list
}

fn restore_snapshot(project_root: &Path, relative_path: &str, snapshot_id: &str) -> Result<(), String> {
    let clean_dir_name = relative_path.replace(['/', '\\'], "__");
    let storage_dir = project_root.join(".betty_history").join(clean_dir_name);
    let backup_path = storage_dir.join(format!("{}.bak", snapshot_id));
    if !backup_path.exists() {
        return Err("Резервная копия не найдена".to_string());
    }
    let target = project_root.join(relative_path);
    if target.exists() {
        let _ = save_snapshot(project_root, relative_path, "Автобэкап перед откатом");
    }
    let content = fs::read(backup_path).map_err(|e| e.to_string())?;
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    fs::write(target, content).map_err(|e| e.to_string())?;
    Ok(())
}

fn get_journal_path(project_root: &Path) -> PathBuf {
    project_root.join(".betty_history").join("chat_journal.jsonl")
}

fn save_journal_entry(project_root: &Path, entry: &ChatJournalEntry) -> Result<(), String> {
    if project_root.as_os_str().is_empty() {
        return Ok(());
    }
    let hist_dir = project_root.join(".betty_history");
    fs::create_dir_all(&hist_dir).map_err(|e| e.to_string())?;
    let journal_path = hist_dir.join("chat_journal.jsonl");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal_path)
        .map_err(|e| e.to_string())?;
    let json_line = serde_json::to_string(entry).map_err(|e| e.to_string())?;
    writeln!(file, "{}", json_line).map_err(|e| e.to_string())?;
    Ok(())
}

fn load_journal_entries(project_root: &Path) -> Vec<ChatJournalEntry> {
    if project_root.as_os_str().is_empty() {
        return Vec::new();
    }
    let journal_path = get_journal_path(project_root);
    if !journal_path.exists() {
        return Vec::new();
    }
    let mut entries = Vec::new();
    if let Ok(file) = fs::File::open(journal_path) {
        let reader = BufReader::new(file);
        for line in reader.lines().flatten() {
            let clean = line.trim();
            if clean.is_empty() {
                continue;
            }
            if let Ok(entry) = serde_json::from_str::<ChatJournalEntry>(clean) {
                entries.push(entry);
            }
        }
    }
    entries
}

fn clear_journal_entries(project_root: &Path) -> Result<(), String> {
    if project_root.as_os_str().is_empty() {
        return Ok(());
    }
    let journal_path = get_journal_path(project_root);
    if journal_path.exists() {
        fs::remove_file(journal_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ----------------------------------------------------------------------------
// PROJECT ECOSYSTEM RUNNER
// ----------------------------------------------------------------------------

pub struct ProjectEcosystem;

impl ProjectEcosystem {
    pub async fn run_loop(
        mut rx: mpsc::UnboundedReceiver<ProjectCmd>,
        tx_to_front: broadcast::Sender<ToFront>,
        tx_observer: mpsc::UnboundedSender<ObserverCmd>,
    ) {
        println!("📁 [Project Ecosystem]: Фабрика управления проектом запущена.");
        let mut current_project = ProjectContextState::empty();

        while let Some(cmd) = rx.recv().await {
            match cmd {
                ProjectCmd::ScanProjects => {
                    let boxes = PathFinder::scan_projects();
                    let _ = tx_to_front.send(ToFront::UpdateAvailableBoxes(boxes));
                }
                ProjectCmd::OpenProject(path) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "Unknown".to_string());
                    current_project = ProjectContextState::load_or_create(&path, &name);
                    current_project.sync_hashes(None);
                    let file_tree_paths: Vec<PathBuf> = current_project
                        .file_tree
                        .iter()
                        .map(PathBuf::from)
                        .collect();
                    let _ = tx_to_front.send(ToFront::ProjectOpened {
                        name,
                        root: current_project.project_root.clone(),
                        file_tree: file_tree_paths,
                    });
                    let journal = load_journal_entries(&current_project.project_root);
                    let _ = tx_to_front.send(ToFront::ChatHistoryLoaded(journal));
                    let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                        TelemetryEnvelope::success("Project", "Проект успешно открыт и проиндексирован")
                    ));
                }
                ProjectCmd::CloseProject => {
                    current_project = ProjectContextState::empty();
                    let _ = tx_to_front.send(ToFront::ProjectClosed);
                    let _ = tx_to_front.send(ToFront::ChatHistoryLoaded(Vec::new()));
                    let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                        TelemetryEnvelope::info("Project", "Проект закрыт")
                    ));
                }
                ProjectCmd::SaveJournalEntry(entry) => {
                    if let Err(e) = save_journal_entry(&current_project.project_root, &entry) {
                        let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                            TelemetryEnvelope::warn("Project", format!("Не удалось сохранить журнал диалога: {}", e))
                        ));
                    }
                }
                ProjectCmd::ClearJournal => {
                    let _ = clear_journal_entries(&current_project.project_root);
                    let _ = tx_to_front.send(ToFront::ChatHistoryLoaded(Vec::new()));
                    let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                        TelemetryEnvelope::info("Project", "История сообщений проекта очищена")
                    ));
                }
                ProjectCmd::CreateFile(rel) => {
                    let target = current_project.project_root.join(&rel);
                    if let Some(parent) = target.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if fs::File::create(&target).is_ok() {
                        current_project.sync_hashes(None);
                        let file_tree_paths: Vec<PathBuf> = current_project
                            .file_tree
                            .iter()
                            .map(PathBuf::from)
                            .collect();
                        let _ = tx_to_front.send(ToFront::ProjectOpened {
                            name: current_project.project_name.clone(),
                            root: current_project.project_root.clone(),
                            file_tree: file_tree_paths,
                        });
                        let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                            TelemetryEnvelope::info("Project", format!("Создан файл: {}", rel))
                        ));
                    }
                }
                ProjectCmd::CreateFolder(rel) => {
                    let target = current_project.project_root.join(&rel);
                    if fs::create_dir_all(&target).is_ok() {
                        current_project.sync_hashes(None);
                        let file_tree_paths: Vec<PathBuf> = current_project
                            .file_tree
                            .iter()
                            .map(PathBuf::from)
                            .collect();
                        let _ = tx_to_front.send(ToFront::ProjectOpened {
                            name: current_project.project_name.clone(),
                            root: current_project.project_root.clone(),
                            file_tree: file_tree_paths,
                        });
                        let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                            TelemetryEnvelope::info("Project", format!("Создана папка: {}", rel))
                        ));
                    }
                }
                ProjectCmd::DeleteFile(rel) => {
                    let target = current_project.project_root.join(&rel);
                    if target.is_file() {
                        let _ = fs::remove_file(&target);
                    } else if target.is_dir() {
                        let _ = fs::remove_dir_all(&target);
                    }
                    current_project.sync_hashes(None);
                    let file_tree_paths: Vec<PathBuf> = current_project
                        .file_tree
                        .iter()
                        .map(PathBuf::from)
                        .collect();
                    let _ = tx_to_front.send(ToFront::ProjectOpened {
                        name: current_project.project_name.clone(),
                        root: current_project.project_root.clone(),
                        file_tree: file_tree_paths,
                    });
                    let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                        TelemetryEnvelope::info("Project", format!("Удалено: {}", rel))
                    ));
                }
                ProjectCmd::GetFileHistory(rel) => {
                    let snaps = list_snapshots(&current_project.project_root, &rel);
                    let _ = tx_to_front.send(ToFront::FileHistoryLoaded {
                        relative_path: rel,
                        snapshots: snaps,
                    });
                }
                ProjectCmd::CreateSnapshot { relative_path, reason } => {
                    match save_snapshot(&current_project.project_root, &relative_path, &reason) {
                        Ok(meta) => {
                            let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                                TelemetryEnvelope::success("Project", format!("Снапшот сохранен: {} [{}]", meta.relative_path, meta.id))
                            ));
                            let snaps = list_snapshots(&current_project.project_root, &relative_path);
                            let _ = tx_to_front.send(ToFront::FileHistoryLoaded {
                                relative_path,
                                snapshots: snaps,
                            });
                        }
                        Err(e) => {
                            let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                                TelemetryEnvelope::error("Project", format!("Ошибка снапшота: {}", e))
                            ));
                        }
                    }
                }
                ProjectCmd::CreateSnapshotForFocus { files, reason } => {
                    let mut count = 0;
                    for f in &files {
                        if save_snapshot(&current_project.project_root, f, &reason).is_ok() {
                            count += 1;
                        }
                    }
                    let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                        TelemetryEnvelope::info("Project", format!("Создан снапшот для {} файлов фокуса", count))
                    ));
                }
                ProjectCmd::RestoreSnapshot { relative_path, snapshot_id } => {
                    match restore_snapshot(&current_project.project_root, &relative_path, &snapshot_id) {
                        Ok(_) => {
                            current_project.sync_hashes(Some(&relative_path));
                            let _ = tx_to_front.send(ToFront::SnapshotRestored {
                                relative_path: relative_path.clone(),
                                snapshot_id: snapshot_id.clone(),
                                success: true,
                                message: format!("Файл {} успешно восстановлен", relative_path),
                            });
                            let snaps = list_snapshots(&current_project.project_root, &relative_path);
                            let _ = tx_to_front.send(ToFront::FileHistoryLoaded {
                                relative_path,
                                snapshots: snaps,
                            });
                        }
                        Err(e) => {
                            let _ = tx_to_front.send(ToFront::SnapshotRestored {
                                relative_path,
                                snapshot_id,
                                success: false,
                                message: format!("Ошибка отката: {}", e),
                            });
                        }
                    }
                }
                ProjectCmd::GetContextForAi { focus, reply_to } => {
                    current_project.sync_hashes(None);
                    let context = current_project.build_context(&focus);
                    let _ = reply_to.send(context);
                }
                ProjectCmd::FetchCodeSlices { queries, reply_to } => {
                    let mut snippets = Vec::new();
                    if !current_project.project_root.as_os_str().is_empty() {
                        for q in &queries {
                            if let Some(snippet) = extract_slice(&current_project.project_root, q) {
                                snippets.push(snippet);
                            }
                        }
                    }
                    let _ = reply_to.send(snippets);
                }
            }
        }
    }
}
