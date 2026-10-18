pub mod api;
pub mod application;
pub mod config;
pub mod domain;
pub mod infrastructure;

use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<config::Config>,
    pub client: reqwest::Client,
    pub capacity: Arc<tokio::sync::Semaphore>,
    pub run_capacity: Arc<tokio::sync::Semaphore>,
}

impl AppState {
    pub fn new(config: config::Config) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(
                config.request_timeout_seconds,
            ))
            .connect_timeout(std::time::Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self {
            capacity: Arc::new(tokio::sync::Semaphore::new(config.max_concurrent_runs)),
            run_capacity: Arc::new(tokio::sync::Semaphore::new(config.max_concurrent_runs)),
            config: Arc::new(config),
            client,
        })
    }
}
