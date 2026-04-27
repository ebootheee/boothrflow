use parking_lot::Mutex;

use crate::error::Result;
use crate::injector::Injector;

/// Records every `inject()` call into an internal Vec. Tests assert against it.
#[derive(Default)]
pub struct RecordingInjector {
    received: Mutex<Vec<String>>,
}

impl RecordingInjector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calls(&self) -> Vec<String> {
        self.received.lock().clone()
    }
}

impl Injector for RecordingInjector {
    fn inject(&self, text: &str) -> Result<()> {
        self.received.lock().push(text.to_string());
        Ok(())
    }

    fn name(&self) -> &str {
        "recording-injector"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_in_order() {
        let inj = RecordingInjector::new();
        inj.inject("first").unwrap();
        inj.inject("second").unwrap();
        assert_eq!(inj.calls(), vec!["first".to_string(), "second".to_string()]);
    }
}
