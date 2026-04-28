use crossbeam_channel::{bounded, Receiver};
use parking_lot::Mutex;

use crate::error::Result;
use crate::hotkey::{HotkeyEvent, HotkeySource};

/// Hand-fed hotkey for tests. Emits the supplied events in order on `start()`,
/// then closes the channel.
pub struct ScriptedHotkey {
    events: Mutex<Vec<HotkeyEvent>>,
}

impl ScriptedHotkey {
    pub fn new(events: Vec<HotkeyEvent>) -> Self {
        Self {
            events: Mutex::new(events),
        }
    }

    pub fn one_press_release() -> Self {
        Self::new(vec![HotkeyEvent::Press, HotkeyEvent::Release])
    }

    pub fn one_quickpaste() -> Self {
        Self::new(vec![HotkeyEvent::QuickPasteOpen])
    }
}

impl HotkeySource for ScriptedHotkey {
    fn start(&self) -> Result<Receiver<HotkeyEvent>> {
        let (tx, rx) = bounded(16);
        for evt in std::mem::take(&mut *self.events.lock()) {
            let _ = tx.send(evt);
        }
        // Dropping tx closes the channel, which signals the consumer that
        // there are no more events.
        drop(tx);
        Ok(rx)
    }

    fn stop(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_emits_in_order_then_closes() {
        let h = ScriptedHotkey::one_press_release();
        let rx = h.start().unwrap();
        let collected: Vec<HotkeyEvent> = rx.iter().collect();
        assert_eq!(collected, vec![HotkeyEvent::Press, HotkeyEvent::Release]);
    }
}
