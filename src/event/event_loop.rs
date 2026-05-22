use crossterm::event::{self, Event, KeyEvent};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub enum AppEvent {
    Tick,
    Key(KeyEvent),
}

pub struct EventLoop {
    pub rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl EventLoop {
    pub fn new(tick_rate: Duration) -> (Self, mpsc::UnboundedSender<AppEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_clone = tx.clone();
        let tx_tick = tx.clone();

        tokio::spawn(async move {
            let mut last_tick = Instant::now();
            loop {
                let timeout = tick_rate
                    .checked_sub(last_tick.elapsed())
                    .unwrap_or(Duration::from_secs(0));

                match event::poll(timeout) {
                    Ok(true) => {
                        if let Ok(Event::Key(key)) = event::read()
                            && tx_clone.send(AppEvent::Key(key)).is_err()
                        {
                            break;
                        }
                    }
                    Ok(false) => {}
                    Err(_) => break,
                }

                if last_tick.elapsed() >= tick_rate {
                    if tx_tick.send(AppEvent::Tick).is_err() {
                        break;
                    }
                    last_tick = Instant::now();
                }
            }
        });

        (Self { rx }, tx)
    }
}
