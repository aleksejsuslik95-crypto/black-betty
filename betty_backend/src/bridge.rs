use std::collections::HashSet;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

// ============================================================================
// 1. WIRE PROTOCOL DTOs (БЭКЕНД <-> ФРОНТЕНД GODOT)
// ============================================================================

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SheetKind {
    FullFile,
    SearchReplace { target: String, replacement: String },
    Snippet,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FocusSelection {
    #[serde(default)]
    pub code_files: HashSet<String>,
    #[serde(default)]
    pub outline_dirs: HashSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSliceQuery {
    pub relative_path: String,
    pub pattern: Option<String>,
    pub start_line: Option<usize>,
    pub line_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FileSliceSnippet {
    pub relative_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

impl FocusSelection {
    pub fn clear(&mut self) {
        self.code_files.clear();
        self.outline_dirs.clear();
    }

    pub fn insert_file(&mut self, path: String) {
        self.code_files.insert(path);
    }

    pub fn remove_file(&mut self, path: &str) {
        self.code_files.remove(path);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodeSheet {
    pub id: String,
    pub raw_content: String,
    pub kind: SheetKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum WorkbenchBlock {
    TextBlock(String),
    SheetBlock(CodeSheet),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatJournalEntry {
    pub id: String,
    pub timestamp: String,
    pub model: String,
    pub user_prompt: String,
    pub betty_reply: String,
    pub blocks: Vec<WorkbenchBlock>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotMeta {
    pub id: String,
    pub relative_path: String,
    pub timestamp: String,
    pub file_size: u64,
    pub hash: String,
    pub reason: String,
}


#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderKind {
    Gemini,
    OpenAiCompatible,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub display_name: String,
    pub provider: ProviderKind,
    pub base_url: Option<String>,
    pub api_key_name: String,
}

fn default_language() -> String {
    "ru".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsData {
    #[serde(default)]
    pub api_keys: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub models: Vec<ModelEntry>,
    #[serde(default)]
    pub active_model: Option<String>,
    #[serde(default = "default_language")]
    pub language: String,
}

impl Default for SettingsData {
    fn default() -> Self {
        Self {
            api_keys: std::collections::HashMap::new(),
            models: Vec::new(),
            active_model: None,
            language: default_language(),
        }
    }
}

impl SettingsData {
    const SETTINGS_FILE: &'static str = "betty_settings.json";

    pub fn load_or_default() -> Self {
        if std::path::Path::new(Self::SETTINGS_FILE).exists() {
            if let Ok(content) = std::fs::read_to_string(Self::SETTINGS_FILE) {
                if let Ok(data) = serde_json::from_str::<Self>(&content) {
                    return data;
                }
            }
        }
        Self::default()
    }

    pub fn save_to_disk(&self) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(Self::SETTINGS_FILE, json)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", content = "data")]
pub enum ToFront {
    ProjectOpened { name: String, root: PathBuf, file_tree: Vec<PathBuf> },
    ProjectClosed,
    UpdateAvailableBoxes(Vec<String>),
    StreamChunk(String),
    StreamEnd,
    SetOutputBlocks(Vec<WorkbenchBlock>),
    UpdateVibe { location: String, atmosphere: String },
    SetStatus(String),
    FileHistoryLoaded { relative_path: String, snapshots: Vec<SnapshotMeta> },
    SnapshotRestored { relative_path: String, snapshot_id: String, success: bool, message: String },
    SettingsLoaded(SettingsData),
    ChatHistoryLoaded(Vec<ChatJournalEntry>),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", content = "data")]
pub enum FromFront {
    ScanProjects,
    OpenProject(PathBuf),
    CloseProject,
    UpdateFocus(FocusSelection),
    OpenFile { path: String, editor: Option<String> },
    CreateFile { relative_path: String },
    CreateFolder { relative_path: String },
    DeleteFile { relative_path: String },
    SubmitInput { text: String, model: String },
    SaveChatEntry(ChatJournalEntry),
    ClearChatHistory,
    TrainBrain { user_msg: String, betty_reply: String },
    UnlockNetwork,
    GetZen,
    OpenTerminal(PathBuf),
    OpenRll { project_path: String },
    OpenCompanion { url: Option<String> },
    GetFileHistory { relative_path: String },
    CreateSnapshot { relative_path: String, reason: String },
    CreateSnapshotForFocus { reason: String },
    RestoreSnapshot { relative_path: String, snapshot_id: String },
    GetSettings,
    SaveSettings(SettingsData),
    SetLocale(String),
}

// ============================================================================
// 2. STANDARDIZED TELEMETRY ENVELOPE
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TelemetryLevel {
    Info,
    Success,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEnvelope {
    pub source: String,
    pub level: TelemetryLevel,
    pub message: String,
}

impl TelemetryEnvelope {
    pub fn info(source: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { source: source.into(), level: TelemetryLevel::Info, message: msg.into() }
    }
    pub fn success(source: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { source: source.into(), level: TelemetryLevel::Success, message: msg.into() }
    }
    pub fn warn(source: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { source: source.into(), level: TelemetryLevel::Warn, message: msg.into() }
    }
    pub fn error(source: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { source: source.into(), level: TelemetryLevel::Error, message: msg.into() }
    }
}

// ============================================================================
// 3. ECOSYSTEM COMMAND ENUMS
// ============================================================================

pub enum ProjectCmd {
    ScanProjects,
    OpenProject(PathBuf),
    CloseProject,
    CreateFile(String),
    CreateFolder(String),
    DeleteFile(String),
    GetFileHistory(String),
    CreateSnapshot { relative_path: String, reason: String },
    CreateSnapshotForFocus { files: Vec<String>, reason: String },
    RestoreSnapshot { relative_path: String, snapshot_id: String },
    SaveJournalEntry(ChatJournalEntry),
    ClearJournal,
    GetContextForAi {
        focus: FocusSelection,
        reply_to: tokio::sync::oneshot::Sender<String>,
    },
    FetchCodeSlices {
        queries: Vec<FileSliceQuery>,
        reply_to: tokio::sync::oneshot::Sender<Vec<FileSliceSnippet>>,
    },
}


pub enum AiCmd {
    SubmitInput {
        text: String,
        model: String,
        dynamic_context: String,
    },
    TrainBrain {
        user_msg: String,
        betty_reply: String,
    },
    GetSettings,
    SaveSettings(SettingsData),
}

pub enum OsCmd {
    OpenFile { path: String, editor: Option<String> },
    OpenTerminal(PathBuf),
    OpenRll { project_path: String },
    OpenCompanion { url: Option<String> },
}

pub enum ObserverCmd {
    RecordTelemetry(TelemetryEnvelope),
    GetZen,
    SetLocale(String),
}

// ============================================================================
// 4. GLOBAL DIPLOMAT STATE
// ============================================================================

pub struct DiplomatState {
    pub active_project: Option<PathBuf>,
    pub current_focus: FocusSelection,
    pub network_busy: bool,
}

impl DiplomatState {
    pub fn new() -> Self {
        Self {
            active_project: None,
            current_focus: FocusSelection::default(),
            network_busy: false,
        }
    }
}

// ============================================================================
// 5. THE FAT DIPLOMAT (CENTRAL ROUTER & DISPATCHER)
// ============================================================================

pub struct DiplomatBridge {
    pub state: DiplomatState,
    pub rx_front: mpsc::UnboundedReceiver<FromFront>,
    pub tx_self: mpsc::UnboundedSender<FromFront>,
    pub tx_project: mpsc::UnboundedSender<ProjectCmd>,
    pub tx_ai: mpsc::UnboundedSender<AiCmd>,
    pub tx_os: mpsc::UnboundedSender<OsCmd>,
    pub tx_observer: mpsc::UnboundedSender<ObserverCmd>,
}
impl DiplomatBridge {
    pub fn new(
        rx_front: mpsc::UnboundedReceiver<FromFront>,
        tx_self: mpsc::UnboundedSender<FromFront>,
        tx_project: mpsc::UnboundedSender<ProjectCmd>,
        tx_ai: mpsc::UnboundedSender<AiCmd>,
        tx_os: mpsc::UnboundedSender<OsCmd>,
        tx_observer: mpsc::UnboundedSender<ObserverCmd>,
    ) -> Self {
        Self {
            state: DiplomatState::new(),
            rx_front,
            tx_self,
            tx_project,
            tx_ai,
            tx_os,
            tx_observer,
        }
    }

    pub async fn run_loop(mut self) {
        println!("🏛️ [The Diplomat]: Диспетчер шины и единого состояния запущен.");

        while let Some(msg) = self.rx_front.recv().await {
            match msg {
                FromFront::ScanProjects => {
                    let _ = self.tx_project.send(ProjectCmd::ScanProjects);
                }
                FromFront::OpenProject(path) => {
                    self.state.active_project = Some(path.clone());
                    self.state.current_focus.clear(); // Теперь работает!
                    let _ = self.tx_project.send(ProjectCmd::OpenProject(path));
                }
                FromFront::CloseProject => {
                    self.state.active_project = None;
                    self.state.current_focus.clear(); // Теперь работает!
                    let _ = self.tx_project.send(ProjectCmd::CloseProject);
                }
                FromFront::UpdateFocus(focus) => {
                    let files_count = focus.code_files.len();
                    let dirs_count = focus.outline_dirs.len();
                    self.state.current_focus = focus;
                    let _ = self.tx_observer.send(ObserverCmd::RecordTelemetry(
                        TelemetryEnvelope::info(
                            "Diplomat",
                            format!("Фокус: {} файлов с кодом, {} папок структуры", files_count, dirs_count)
                        )
                    ));
                }
                FromFront::OpenFile { path, editor } => {
                    let _ = self.tx_os.send(OsCmd::OpenFile { path, editor });
                }
                FromFront::CreateFile { relative_path } => {
                    let clean_path = relative_path.replace('\\', "/");
                    self.state.current_focus.insert_file(clean_path);
                    let _ = self.tx_project.send(ProjectCmd::CreateFile(relative_path));
                }
                FromFront::CreateFolder { relative_path } => {
                    let _ = self.tx_project.send(ProjectCmd::CreateFolder(relative_path));
                }
                FromFront::DeleteFile { relative_path } => {
                    let clean_path = relative_path.replace('\\', "/");
                    self.state.current_focus.remove_file(&clean_path);
                    let _ = self.tx_project.send(ProjectCmd::DeleteFile(relative_path));
                }
                FromFront::SubmitInput { text, model } => {
                    if self.state.network_busy {
                        let _ = self.tx_observer.send(ObserverCmd::RecordTelemetry(
                            TelemetryEnvelope::warn("Diplomat", "Шлюз занят, комбо-запрос отклонен.")
                        ));
                        continue;
                    }
                    self.state.network_busy = true;

                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                    let _ = self.tx_project.send(ProjectCmd::GetContextForAi {
                        focus: self.state.current_focus.clone(),
                        reply_to: reply_tx,
                    });

                    let tx_ai = self.tx_ai.clone();
                    let tx_self = self.tx_self.clone();
                    tokio::spawn(async move {
                        let dynamic_context = reply_rx.await.unwrap_or_default();
                        let _ = tx_ai.send(AiCmd::SubmitInput {
                            text,
                            model,
                            dynamic_context,
                        });
                        let _ = tx_self;
                    });
                }
                FromFront::TrainBrain { user_msg, betty_reply } => {
                    let _ = self.tx_ai.send(AiCmd::TrainBrain { user_msg, betty_reply });
                }
                FromFront::GetSettings => {
                    let _ = self.tx_ai.send(AiCmd::GetSettings);
                }
                FromFront::SaveSettings(settings) => {
                    if !settings.language.is_empty() {
                        let _ = self.tx_observer.send(ObserverCmd::SetLocale(settings.language.clone()));
                    }
                    let _ = self.tx_ai.send(AiCmd::SaveSettings(settings));
                }
                FromFront::SetLocale(locale) => {
                    let _ = self.tx_observer.send(ObserverCmd::SetLocale(locale));
                }
                FromFront::UnlockNetwork => {
                    self.state.network_busy = false;
                }
                FromFront::GetZen => {
                    let _ = self.tx_observer.send(ObserverCmd::GetZen);
                }
                FromFront::OpenTerminal(path) => {
                    let _ = self.tx_os.send(OsCmd::OpenTerminal(path));
                }
                FromFront::OpenRll { project_path } => {
                    let _ = self.tx_os.send(OsCmd::OpenRll { project_path });
                }
                FromFront::OpenCompanion { url } => {
                    let _ = self.tx_os.send(OsCmd::OpenCompanion { url });
                }
                FromFront::GetFileHistory { relative_path } => {
                    let _ = self.tx_project.send(ProjectCmd::GetFileHistory(relative_path));
                }
                FromFront::CreateSnapshot { relative_path, reason } => {
                    let _ = self.tx_project.send(ProjectCmd::CreateSnapshot { relative_path, reason });
                }
                FromFront::CreateSnapshotForFocus { reason } => {
                    let files: Vec<String> = self.state.current_focus
                        .code_files
                        .iter()
                        .cloned()
                        .collect();
                    let _ = self.tx_project.send(ProjectCmd::CreateSnapshotForFocus { files, reason });
                }
                FromFront::RestoreSnapshot { relative_path, snapshot_id } => {
                    let _ = self.tx_project.send(ProjectCmd::RestoreSnapshot { relative_path, snapshot_id });
                }
                FromFront::SaveChatEntry(entry) => {
                    let _ = self.tx_project.send(ProjectCmd::SaveJournalEntry(entry));
                }
                FromFront::ClearChatHistory => {
                    let _ = self.tx_project.send(ProjectCmd::ClearJournal);
                }
            }
        }
        println!("🏛️ [The Diplomat]: Цикл диспетчера завершён.");
    }
}

