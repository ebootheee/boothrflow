//! Typing fallback — types each character via SendInput. Slower than
//! clipboard paste (~50ms/word) but bypasses paste-blocked password fields
//! and apps with strict clipboard policies.

use enigo::{Enigo, Keyboard, Settings};
use parking_lot::Mutex;

use crate::error::{BoothError, Result};
use crate::injector::Injector;

pub struct TypingInjector {
    enigo: Mutex<Enigo>,
}

impl TypingInjector {
    pub fn new() -> Result<Self> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| BoothError::Injection(format!("init enigo: {e}")))?;
        Ok(Self {
            enigo: Mutex::new(enigo),
        })
    }
}

impl Injector for TypingInjector {
    fn inject(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        self.enigo
            .lock()
            .text(text)
            .map_err(|e| BoothError::Injection(format!("type text: {e}")))?;
        Ok(())
    }

    fn name(&self) -> &str {
        "typing-injector"
    }
}
