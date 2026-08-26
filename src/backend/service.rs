use std::sync::Arc;

use chrono_tz::Tz;
use tokio::sync::Semaphore;

use crate::backend::{db::Repository, image::processor::ImageProcessor, storage::Storage};

pub mod image;

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
