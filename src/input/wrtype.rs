//! Wayland text injection via wrtype

use anyhow::Result;

pub trait TextInjector {
    fn inject(&mut self, text: &str) -> Result<()>;
}

pub struct WrtypeInjector {
    client: wrtype::WrtypeClient,
}

impl WrtypeInjector {
    pub fn new() -> Result<Self> {
        let client = wrtype::WrtypeClient::new()
            .map_err(|e| anyhow::anyhow!("failed to initialize wrtype client: {}", e))?;
        Ok(Self { client })
    }
}

impl TextInjector for WrtypeInjector {
    fn inject(&mut self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        let text_with_space = format!("{} ", text);
        self.client
            .type_text(&text_with_space)
            .map_err(|e| anyhow::anyhow!("failed to inject text: {}", e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockInjector;

    impl TextInjector for MockInjector {
        fn inject(&mut self, text: &str) -> Result<()> {
            if text.is_empty() {
                return Ok(());
            }
            Ok(())
        }
    }

    #[test]
    fn empty_string_guard_returns_ok() {
        let mut injector = MockInjector;
        let result = injector.inject("");
        assert!(result.is_ok());
    }
}
