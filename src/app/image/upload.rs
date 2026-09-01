use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;

use crate::backend::{
    error::{ImageProcessorError, ServiceError, StorageError},
    service::Service,
};

/// 将请求体分块写入磁盘，内存中只保留当前网络分块。
pub(crate) struct StreamingUpload {
    temp_file: NamedTempFile,
    writer: tokio::fs::File,
    length: usize,
    max_length: usize,
}

impl StreamingUpload {
    pub(crate) fn new(service: &Service) -> Result<Self, ServiceError> {
        let temp_file = service.create_upload_file()?;
        let writer = tokio::fs::File::from_std(temp_file.reopen().map_err(StorageError::from)?);

        Ok(Self {
            temp_file,
            writer,
            length: 0,
            max_length: service.max_upload_size(),
        })
    }

    pub(crate) async fn write(&mut self, chunk: &[u8]) -> Result<(), ServiceError> {
        let next_length = self
            .length
            .checked_add(chunk.len())
            .ok_or(ImageProcessorError::TooLarge)?;

        if next_length > self.max_length {
            return Err(ImageProcessorError::TooLarge.into());
        }

        self.writer
            .write_all(chunk)
            .await
            .map_err(StorageError::from)?;
        self.length = next_length;
        Ok(())
    }

    pub(crate) async fn finish(mut self) -> Result<(NamedTempFile, usize), ServiceError> {
        if self.length == 0 {
            return Err(ImageProcessorError::EmptyInput.into());
        }

        self.writer.flush().await.map_err(StorageError::from)?;
        drop(self.writer);
        Ok((self.temp_file, self.length))
    }
}
