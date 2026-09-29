#[derive(Clone)]
pub struct Handler;

impl Handler {
    pub async fn handle(&self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}
