use crate::context::{AppContext, ContextDetector};

pub struct FixedContextDetector(pub Option<AppContext>);

impl FixedContextDetector {
    pub fn slack() -> Self {
        Self(Some(AppContext {
            app_exe: "slack.exe".into(),
            window_title: Some("general — Acme".into()),
            control_role: Some("Edit".into()),
        }))
    }

    pub fn none() -> Self {
        Self(None)
    }
}

impl ContextDetector for FixedContextDetector {
    fn detect(&self) -> Option<AppContext> {
        self.0.clone()
    }
}
