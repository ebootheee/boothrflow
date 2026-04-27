use crate::hotkey::HotkeyEvent;

/// Hand-fed hotkey for tests. `next()` plays back events in order.
pub struct ScriptedHotkey {
    events: Vec<HotkeyEvent>,
    cursor: std::sync::atomic::AtomicUsize,
}

impl ScriptedHotkey {
    pub fn new(events: Vec<HotkeyEvent>) -> Self {
        Self {
            events,
            cursor: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn one_press_release() -> Self {
        Self::new(vec![HotkeyEvent::Press, HotkeyEvent::Release])
    }

    pub fn next(&self) -> Option<HotkeyEvent> {
        let i = self
            .cursor
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.events.get(i).copied()
    }
}
