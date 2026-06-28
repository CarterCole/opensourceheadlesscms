use std::collections::HashMap;
use crate::core::error::Result;

#[derive(Debug, Clone)]
pub struct RenderedComponent {
    pub html: String,
}

pub trait ComponentHandler: Send + Sync {
    fn render(&self, props: &HashMap<String, String>) -> Result<RenderedComponent>;
}

pub trait ComponentRegistry: Send + Sync {
    fn resolve(&self, name: &str, props: &HashMap<String, String>) -> Result<RenderedComponent>;
    fn register(&mut self, name: &str, handler: Box<dyn ComponentHandler>);
    fn known_components(&self) -> Vec<&str>;
}
