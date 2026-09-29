mod bridge;
mod ecosystems;
mod net;
mod tools;

use std::env;
use tokio::sync::{broadcast, mpsc};

use bridge::{AiCmd, DiplomatBridge, FromFront, ObserverCmd, OsCmd, ProjectCmd, ToFront};
use ecosystems::ai_ecosystem::AiEcosystem;
use ecosystems::observer_ecosystem::ObserverEcosystem;
use ecosystems::os_ecosystem::OsEcosystem;
use ecosystems::project_ecosystem::ProjectEcosystem;
use net::ws_server::WsBridgeServer;

#[tokio::main]
async fn main() {
    println!("🔥 [The Bootstrapper]: Инициализация экосистем Бэтти...");

    // Читаем .env
    if let Ok(path) = dotenvy::dotenv() {
        println!("📄 [The Bootstrapper]: .env загружен: {:?}", path);
    }
    if let Ok(raw_key) = env::var("GEMINI_API_KEY") {
        let clean_key = raw_key.trim().trim_matches('"').trim_matches('\'').to_string();
        env::set_var("GEMINI_API_KEY", clean_key);
    }

    // Определяем порт WebSocket
    let args: Vec<String> = env::args().collect();
    let mut port: u16 = 8765;
    if let Some(pos) = args.iter().position(|r| r == "--port") {
        if let Some(p_str) = args.get(pos + 1) {
            if let Ok(parsed_port) = p_str.parse::<u16>() {
                port = parsed_port;
            }
        }
    }
    let bind_addr = format!("127.0.0.1:{}", port);

    // ------------------------------------------------------------------------
    // WIRING CHANNELS (ТОПОЛОГИЯ ШИНЫ)
    // ------------------------------------------------------------------------

    // Внешняя шина (WebSocket <-> Diplomat)
    let (tx_to_front, _) = broadcast::channel::<ToFront>(256);
    let (tx_from_front, rx_from_front) = mpsc::unbounded_channel::<FromFront>();

    // Внутренние доменные каналы (Diplomat -> Ecosystems)
    let (tx_project, rx_project) = mpsc::unbounded_channel::<ProjectCmd>();
    let (tx_ai, rx_ai) = mpsc::unbounded_channel::<AiCmd>();
    let (tx_os, rx_os) = mpsc::unbounded_channel::<OsCmd>();
    let (tx_observer, rx_observer) = mpsc::unbounded_channel::<ObserverCmd>();

    // Инициализируем Observer сохраненным языком из конфига
    let initial_settings = bridge::SettingsData::load_or_default();
    let _ = tx_observer.send(ObserverCmd::SetLocale(initial_settings.language));

    let tx_front_observer = tx_to_front.clone();
    tokio::spawn(async move {
        ObserverEcosystem::run_loop(rx_observer, tx_front_observer).await;
    });

    // 2. Project Ecosystem (Проекты, Файлы, Снапшоты)
    let tx_front_project = tx_to_front.clone();
    let tx_obs_project = tx_observer.clone();
    tokio::spawn(async move {
        ProjectEcosystem::run_loop(rx_project, tx_front_project, tx_obs_project).await;
    });

    // 3. AI Ecosystem (Gemini Шлюз, Память диалога)
    let tx_front_ai = tx_to_front.clone();
    let tx_dip_ai = tx_from_front.clone();
    let tx_obs_ai = tx_observer.clone();
    let tx_proj_ai = tx_project.clone(); // <--- ДОБАВЛЯЕМ КЛОН
    let tx_ai_init = tx_ai.clone();
    tokio::spawn(async move {
        AiEcosystem::run_loop(rx_ai, tx_front_ai, tx_dip_ai, tx_obs_ai, tx_proj_ai).await;
    });
    // Запрашиваем пуш настроек в шину
    let _ = tx_ai_init.send(AiCmd::GetSettings);

    // 4. OS Ecosystem (Внешние процессы, Lite-XL, Терминал)
    let tx_front_os = tx_to_front.clone();
    let tx_obs_os = tx_observer.clone();
    tokio::spawn(async move {
        OsEcosystem::run_loop(rx_os, tx_front_os, tx_obs_os).await;
    });

    // 5. The Fat Diplomat (Bridge Router)
let diplomat = DiplomatBridge::new(
        rx_from_front,
        tx_from_front.clone(),
        tx_project,
        tx_ai,
        tx_os,
        tx_observer,
    );
    tokio::spawn(async move {
        diplomat.run_loop().await;
    });

    // ------------------------------------------------------------------------
    // START TRANSPORT
    // ------------------------------------------------------------------------
    println!("🚀 [The Bootstrapper]: Все домены запущены. Старт сетевого транспорта...");
    WsBridgeServer::start(&bind_addr, tx_from_front, tx_to_front).await;
}
