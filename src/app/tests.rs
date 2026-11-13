use std::{sync::Arc, time::Duration};

use http_body_util::BodyExt;
use image::{ColorType, ImageEncoder, codecs::png::PngEncoder};
use sqlx::SqlitePool;
use tempfile::{TempDir, tempdir};
use tokio::sync::Semaphore;
use topcoat::{
    context::Cx,
    router::{
        Body, Router, StatusCode,
        content::Json,
        request::Request,
        response::{IntoResponse, Response},
    },
};

use crate::{
    auth::service::AuthService,
    config::{AuthConfig, ImageConfig},
    images::{
        model::UploadImage, processing::ImageProcessor, repository::Repository, service::Service,
        storage::Storage,
    },
    server::AppState,
};

const ORIGIN: &str = "http://localhost:8080";
const TOKEN: &str = "0123456789abcdef0123456789abcdef";

fn fixture(pool: SqlitePool) -> (TempDir, Arc<AppState>, Router) {
    let directory = tempdir().unwrap();
    let state = Arc::new(AppState {
        images: Service::new(
            Repository::new(pool),
            ImageProcessor::new(ImageConfig {
                max_upload_size: 1024 * 1024,
                max_pixels: 1_000_000,
                quality: 82.0,
                thumbnail_quality: 75.0,
                method: 4,
                thumbnail_max_edge: 512,
                max_concurrent_processing: 1,
            }),
            Storage::new(directory.path()).unwrap(),
            chrono_tz::Asia::Shanghai,
        ),
        auth: AuthService::new(
            AuthConfig {
                username: "admin".to_owned(),
                password: "test-password".to_owned(),
                token: TOKEN.to_owned(),
            },
            ORIGIN,
        )
        .unwrap(),
        request_limit: Arc::new(Semaphore::new(8)),
        request_timeout: Duration::from_secs(5),
        max_upload_bytes: 2 * 1024 * 1024,
    });
    let router = super::routes(Arc::clone(&state)).build();
    (directory, state, router)
}

fn request(method: &str, path: &str, body: impl Into<Body>) -> Request {
    Request::builder()
        .method(method)
        .uri(path)
        .header("host", "localhost:8080")
        .body(body.into())
        .unwrap()
}

async fn login(router: &Router) -> String {
    let mut request = request("POST", "/login", "username=admin&password=test-password");
    request.headers_mut().insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    request
        .headers_mut()
        .insert("origin", ORIGIN.parse().unwrap());
    let response = router.handle(request).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert_eq!(response.headers()["location"], "/gallery");
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    cookie.split(';').next().unwrap().to_owned()
}

async fn action(router: &Router, cookie: &str, operation: &str, id: &str) -> Response {
    let body = Json((operation, id))
        .into_response(&Cx::new())
        .unwrap()
        .into_body();
    let mut request = request("POST", "/actions/images", body);
    request
        .headers_mut()
        .insert("origin", ORIGIN.parse().unwrap());
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    request
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    router.handle(request).await
}

#[sqlx::test]
async fn authenticates_and_manages_uploaded_images_through_http(pool: SqlitePool) {
    let (_directory, _state, router) = fixture(pool);
    let cookie = login(&router).await;
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(&[255, 0, 0, 0, 255, 0], 2, 1, ColorType::Rgb8.into())
        .unwrap();
    let mut upload = request("POST", "/api/upload", png.clone());
    upload
        .headers_mut()
        .insert("authorization", format!("Bearer {TOKEN}").parse().unwrap());
    upload
        .headers_mut()
        .insert("x-filename", "image.png".parse().unwrap());
    let response = router.handle(upload).await;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let Json(uploaded) = Json::<UploadImage>::from_bytes(&bytes).unwrap();
    assert!(!uploaded.already_exists);
    let id = uploaded.image.public_id.to_string();
    let public_path = format!("/i/{id}");
    let response = router.handle(request("GET", &public_path, ())).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["content-type"], "image/webp");
    let image = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&image[..4], b"RIFF");

    let media_path = format!("/media/active/thumbnail/{id}");
    assert_eq!(
        router
            .handle(request("GET", &media_path, ()))
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let mut media = request("GET", &media_path, ());
    media
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    assert_eq!(router.handle(media).await.status(), StatusCode::OK);

    assert_eq!(
        action(&router, &cookie, "trash", &id).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .handle(request("GET", &public_path, ()))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        action(&router, &cookie, "restore", &id).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .handle(request("GET", &public_path, ()))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        action(&router, &cookie, "trash", &id).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        action(&router, &cookie, "delete", &id).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        router
            .handle(request("GET", &public_path, ()))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );

    let mut multipart = request("POST", "/upload", [
        b"--lensy\r\nContent-Disposition: form-data; name=\"file\"; filename=\"browser.png\"\r\nContent-Type: image/png\r\n\r\n".as_slice(),
        &png, b"\r\n--lensy--\r\n",
    ].concat());
    multipart
        .headers_mut()
        .insert("origin", ORIGIN.parse().unwrap());
    multipart
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    multipart.headers_mut().insert(
        "content-type",
        "multipart/form-data; boundary=lensy".parse().unwrap(),
    );
    let response = router.handle(multipart).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .starts_with("lensy_notice=")
    );
}

#[sqlx::test]
async fn rejects_untrusted_requests_and_enforces_upload_and_concurrency_limits(pool: SqlitePool) {
    let (_directory, state, router) = fixture(pool);
    let response = router.handle(request("POST", "/api/upload", "x")).await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()["www-authenticate"], "Bearer");

    let cookie = login(&router).await;
    let mut forged = request("POST", "/actions/images", r#"["trash","A00000000001"]"#);
    forged
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    forged
        .headers_mut()
        .insert("origin", "https://attacker.example".parse().unwrap());
    forged
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    assert_eq!(router.handle(forged).await.status(), StatusCode::FORBIDDEN);

    let mut oversized = request("POST", "/api/upload", vec![0; 1024 * 1024 + 1]);
    oversized
        .headers_mut()
        .insert("authorization", format!("Bearer {TOKEN}").parse().unwrap());
    assert_eq!(
        router.handle(oversized).await.status(),
        StatusCode::PAYLOAD_TOO_LARGE
    );

    let mut shard = request("POST", "/fragments/images", r#"{"args":[false]}"#);
    shard
        .headers_mut()
        .insert("origin", ORIGIN.parse().unwrap());
    shard
        .headers_mut()
        .insert("content-type", "application/json".parse().unwrap());
    assert_eq!(
        router.handle(shard).await.status(),
        StatusCode::UNAUTHORIZED
    );

    let mut logout = request("POST", "/logout", ());
    logout
        .headers_mut()
        .insert("origin", ORIGIN.parse().unwrap());
    logout
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    assert_eq!(router.handle(logout).await.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        action(&router, &cookie, "trash", "A00000000001")
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );

    let _permits = state
        .request_limit
        .clone()
        .acquire_many_owned(8)
        .await
        .unwrap();
    assert_eq!(
        router
            .handle(request("GET", "/i/A00000000001", ()))
            .await
            .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[sqlx::test]
async fn protects_lazy_image_batches_and_validates_the_cursor(pool: SqlitePool) {
    use topcoat::runtime::{Arguments, Surrogated};

    #[derive(serde::Serialize)]
    struct ShardBody<A> {
        args: A,
    }

    let (_directory, _state, router) = fixture(pool);
    let cookie = login(&router).await;
    for (timestamp, id, authenticated, expected) in [
        (0i64, 0i64, false, StatusCode::UNAUTHORIZED),
        (-1, 1, true, StatusCode::BAD_REQUEST),
        (1, -1, true, StatusCode::BAD_REQUEST),
        (1, 0, true, StatusCode::BAD_REQUEST),
        (0, 0, true, StatusCode::OK),
    ] {
        let body = Json(ShardBody {
            args: Arguments((false, timestamp, id, false).into_surrogate()),
        })
        .into_response(&Cx::new())
        .unwrap()
        .into_body();
        let mut batch = request("POST", "/fragments/images/batch", body);
        batch
            .headers_mut()
            .insert("origin", ORIGIN.parse().unwrap());
        batch
            .headers_mut()
            .insert("content-type", "application/json".parse().unwrap());
        if authenticated {
            batch
                .headers_mut()
                .insert("cookie", cookie.parse().unwrap());
        }
        let response = router.handle(batch).await;
        assert_eq!(response.status(), expected);
        if expected == StatusCode::OK {
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let html = std::str::from_utf8(&bytes).unwrap();
            assert!(html.contains("加载更多"));
            assert!(!html.contains("回收站为空"));
            assert!(!html.contains("<article"));
        }
    }
}
