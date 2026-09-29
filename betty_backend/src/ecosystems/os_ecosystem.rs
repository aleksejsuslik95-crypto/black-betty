use std::path::PathBuf;
use std::process::Command;
use tokio::sync::{broadcast, mpsc};
use crate::bridge::{ObserverCmd, OsCmd, TelemetryEnvelope, ToFront};
use crate::tools::path_finder::PathFinder;

pub struct OsEcosystem;

impl OsEcosystem {
    pub async fn run_loop(
        mut rx: mpsc::UnboundedReceiver<OsCmd>,
        tx_to_front: broadcast::Sender<ToFront>,
        tx_observer: mpsc::UnboundedSender<ObserverCmd>,
    ) {
        println!("⚙️ [OS Ecosystem]: Домен управления внешними процессами запущен.");

        while let Some(cmd) = rx.recv().await {
            match cmd {
                OsCmd::OpenFile { path, editor } => {
                    Self::open_file(&path, editor.as_deref());
                }
                OsCmd::OpenTerminal(path) => {
                    Self::open_terminal(&path, &tx_observer);
                }
                OsCmd::OpenRll { project_path } => {
                    Self::open_rll(&project_path, &tx_to_front, &tx_observer);
                }
                OsCmd::OpenCompanion { url } => {
                    Self::open_companion(url, &tx_to_front, &tx_observer);
                }
            }
        }
    }

    fn open_file(path_str: &str, editor_mode: Option<&str>) {
        let mode = editor_mode.unwrap_or("lite_xl");
        if mode == "lite_xl" {
            let tools_dir = PathFinder::get_tools_root();
            #[cfg(target_os = "windows")]
            let bin = tools_dir.join("lite-xl/lite-xl.exe");
            #[cfg(not(target_os = "windows"))]
            let bin = tools_dir.join("lite-xl/lite-xl");

            if bin.exists() {
                let _ = Command::new(bin).arg(path_str).spawn();
                return;
            }
        }

        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("cmd").args(["/C", "start", "", path_str]).spawn();
        }
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open").arg(path_str).spawn();
        }
        #[cfg(target_os = "linux")]
        {
            let _ = Command::new("xdg-open").arg(path_str).spawn();
        }
    }

    fn open_terminal(path: &PathBuf, tx_observer: &mpsc::UnboundedSender<ObserverCmd>) {
        #[cfg(target_os = "windows")]
        {
            let res = Command::new("cmd")
                .args(["/C", "start", "cmd.exe", "/K"])
                .current_dir(path)
                .spawn();
            if let Err(e) = res {
                let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                    TelemetryEnvelope::error("OS", format!("Не удалось открыть терминал: {}", e))
                ));
            }
        }
        #[cfg(target_os = "macos")]
        {
            let _ = Command::new("open")
                .args(["-a", "Terminal", path.to_str().unwrap_or(".")])
                .spawn();
        }
        #[cfg(target_os = "linux")]
        {
            let _ = Command::new("x-terminal-emulator")
                .current_dir(path)
                .spawn();
        }
    }

    fn open_rll(
        project_path_str: &str,
        tx_to_front: &broadcast::Sender<ToFront>,
        tx_observer: &mpsc::UnboundedSender<ObserverCmd>,
    ) {
        let tools_dir = PathFinder::get_tools_root();
        #[cfg(target_os = "windows")]
        let bin = tools_dir.join("RLLTool/RLL.exe");
        #[cfg(not(target_os = "windows"))]
        let bin = tools_dir.join("RLLTool/RLL");

        if bin.exists() {
            let mut cmd = Command::new(&bin);
            if !project_path_str.is_empty() {
                cmd.arg(project_path_str);
            }
            match cmd.spawn() {
                Ok(_) => {
                    let _ = tx_to_front.send(ToFront::SetStatus("Дебаггер RLL успешно запущен".to_string()));
                }
                Err(e) => {
                    let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                        TelemetryEnvelope::error("OS", format!("Ошибка запуска RLL: {}", e))
                    ));
                }
            }
        } else {
            let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                TelemetryEnvelope::warn("OS", "Исполняемый файл RLLTool не найден в tools/RLLTool/")
            ));
        }
    }

    fn open_companion(
        url: Option<String>,
        tx_to_front: &broadcast::Sender<ToFront>,
        tx_observer: &mpsc::UnboundedSender<ObserverCmd>,
    ) {
        let tools_dir = PathFinder::get_tools_root();
        #[cfg(target_os = "windows")]
        let candidates = [
            tools_dir.join("betty_companion/betty_companion.exe"),
            tools_dir.join("betty_companion/target/release/betty_companion.exe"),
            PathBuf::from("target/release/betty_companion.exe"),
        ];
        #[cfg(not(target_os = "windows"))]
        let candidates = [
            tools_dir.join("betty_companion/betty_companion"),
            tools_dir.join("betty_companion/target/release/betty_companion"),
            PathBuf::from("target/release/betty_companion"),
        ];

        let companion_bin = candidates.into_iter().find(|p| p.exists());

        if let Some(bin) = companion_bin {
            let mut cmd = Command::new(&bin);
            if let Some(ref target_url) = url {
                cmd.arg(target_url);
            }
            match cmd.spawn() {
                Ok(_) => {
                    let _ = tx_to_front.send(ToFront::SetStatus("🌐 Gemini Satellite WebView запущен".to_string()));
                }
                Err(e) => {
                    let _ = tx_observer.send(ObserverCmd::RecordTelemetry(
                        TelemetryEnvelope::error("OS", format!("Ошибка запуска Gemini Companion: {}", e))
                    ));
                }
            }
        } else {
            let target_url = url.unwrap_or_else(|| "https://gemini.google.com".to_string());
            #[cfg(target_os = "windows")]
            {
                let _ = Command::new("cmd").args(["/C", "start", &target_url]).spawn();
            }
            #[cfg(target_os = "macos")]
            {
                let _ = Command::new("open").arg(&target_url).spawn();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = Command::new("xdg-open").arg(&target_url).spawn();
            }
            let _ = tx_to_front.send(ToFront::SetStatus("🌐 Gemini открыт в системном браузере".to_string()));
        }
    }
}
