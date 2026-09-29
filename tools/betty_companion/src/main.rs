use std::path::PathBuf;
use tao::{
    dpi::LogicalSize,
    event::{Event, StartCause, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};
use wry::{WebContext, WebViewBuilder};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://gemini.google.com".to_string());

    // Профиль с куками сохраняется рядом с бинарником или в рабочей папке
    let profile_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".betty_browser_profile");

    let _ = std::fs::create_dir_all(&profile_dir);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("🦊 Betty Web Companion — Gemini")
        .with_inner_size(LogicalSize::new(1280.0, 850.0))
        .build(&event_loop)?;

    let mut web_context = WebContext::new(Some(profile_dir));
    let _webview = WebViewBuilder::with_web_context(&mut web_context)
        .with_url(&target_url)
        .build(&window)?;


    println!("🌐 [Betty-Companion]: Запущен WebView на {}", target_url);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                println!("⚡ [Betty-Companion]: Окно инициализировано.");
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            _ => (),
        }
    });
}
