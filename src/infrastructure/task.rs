/// Dropping a response stream or request future must also cancel its background run.
pub struct AbortOnDrop(pub tokio::task::AbortHandle);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}
