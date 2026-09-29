use rand::seq::SliceRandom;
use tokio::sync::{broadcast, mpsc};
use crate::bridge::{ObserverCmd, TelemetryEnvelope, TelemetryLevel, ToFront};

pub struct ObserverEcosystem;

impl ObserverEcosystem {
    const WISDOM_RU: &'static [&'static str] = &[
        "Прибой не спешит, и ты не должен. Сделай вдох, Бро.",
        "Лед в клубничной воде тает — это и есть life. Наблюдай.",
        "Конфликт — это просто шум в системе. Выбери нашу тишину.",
        "Ты не обязан быть гениальной ячейкой для системы. Ты обязан быть собой.",
        "Смотри на горизонт. Там нет гонки по головам, только чистый свет.",
    ];

    const WISDOM_EN: &'static [&'static str] = &[
        "The tide is in no rush, and neither should you. Breathe in, Bro.",
        "Ice melts in strawberry water — that's life. Just observe.",
        "Conflict is just noise in the system. Choose our silence.",
        "You don't have to be a genius cog for the system. You just have to be yourself.",
        "Look at the horizon. No rat race there, just pure light.",
    ];

    pub fn get_wisdom(locale: &str) -> &'static str {
        let mut rng = rand::thread_rng();
        let pool = if locale.eq_ignore_ascii_case("en") {
            Self::WISDOM_EN
        } else {
            Self::WISDOM_RU
        };
        let fallback = if locale.eq_ignore_ascii_case("en") {
            "The gates are open. Breathe out."
        } else {
            "Ворота открыты. Выдохни."
        };
        *pool.choose(&mut rng).unwrap_or(&fallback)
    }

    pub async fn run_loop(
        mut rx: mpsc::UnboundedReceiver<ObserverCmd>,
        tx_to_front: broadcast::Sender<ToFront>,
    ) {
        println!("👁️ [Observer Ecosystem]: Фабрика телеметрии и локализации запущена.");
        let mut current_locale = "ru".to_string();

        while let Some(cmd) = rx.recv().await {
            match cmd {
                ObserverCmd::SetLocale(locale) => {
                    println!("🌐 [Observer Ecosystem]: Локаль переключена на -> '{}'", locale);
                    current_locale = locale;
                }
                ObserverCmd::RecordTelemetry(envelope) => {
                    Self::process_telemetry(&envelope, &tx_to_front);
                }
                ObserverCmd::GetZen => {
                    let quote = Self::get_wisdom(&current_locale);
                    let _ = tx_to_front.send(ToFront::SetStatus(quote.to_string()));
                }
            }
        }
    }

    fn process_telemetry(envelope: &TelemetryEnvelope, tx_to_front: &broadcast::Sender<ToFront>) {
        let prefix = match envelope.level {
            TelemetryLevel::Info => "ℹ️",
            TelemetryLevel::Success => "✅",
            TelemetryLevel::Warn => "⚠️",
            TelemetryLevel::Error => "🛑",
        };
        let formatted = format!("{} [{}] {}", prefix, envelope.source, envelope.message);
        println!("{}", formatted);
        let _ = tx_to_front.send(ToFront::SetStatus(envelope.message.clone()));
    }
}

