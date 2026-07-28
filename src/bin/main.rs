use std::{env, error::Error};

use lensy::storage::{database::create_pool, image_repo::ImageRepository};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tokio::fs::create_dir_all("./data").await?;

    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/lensy.db".to_owned());

    let pool = create_pool(&db_url).await?;
    let _image_repo = ImageRepository::new(pool);
    println!("database initialized");
    Ok(())
}
