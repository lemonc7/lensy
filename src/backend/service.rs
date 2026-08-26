pub mod api_token;
pub mod image;

use crate::backend::{db::Repository, image::processor::ImageProcessor, storage::Storage};
use chrono_tz::Tz;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct Service {
    repository: Repository,
    processor: Arc<ImageProcessor>,
    storage: Arc<Storage>,
    processing_limit: Arc<Semaphore>,
    timezone: Tz,
}

impl Service {
    pub fn new(
        repository: Repository,
        processor: ImageProcessor,
        storage: Storage,
        timezone: Tz,
    ) -> Self {
        let max_concurrent_processing = processor.max_concurrent_processing();
        Self {
            repository,
            processor: Arc::new(processor),
            storage: Arc::new(storage),
            processing_limit: Arc::new(Semaphore::new(max_concurrent_processing)),
            timezone,
        }
    }
}
