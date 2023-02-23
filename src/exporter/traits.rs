use async_trait::async_trait;

pub trait Collector {
    fn collect(&mut self) -> Result<String, ()>;
}
