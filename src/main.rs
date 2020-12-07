
use actix_web::{
    web, dev::BodyEncoding, get, http::ContentEncoding, middleware, App, HttpResponse, HttpServer,
};

// use serde::serde_derive::Deserialize;
use serde::{Serialize, Deserialize};


use image::io::Reader as ImageReader;
use image::DynamicImage;
use image::ImageError;
use image::ImageOutputFormat::{Png};

fn image_bytes(filename: &str) -> Result<Vec<u8>, ImageError> {
    let mut buffer: Vec<u8> = Vec::new();
    let img = ImageReader::open(filename)?.decode()?;
    img.write_to(&mut buffer, Png)?;

    Ok(buffer)
}

#[derive(Deserialize)]
struct Info {
    filename: String,
    extension: String,
}

async fn index(info: web::Path<Info>) -> HttpResponse {
    let filename = &info.filename;
    let extension = &info.extension;

    let full_filename = format!("{}.{}", &filename, &extension);

    match image_bytes(&full_filename) {
        Ok(buffer) => {
            HttpResponse::Ok()
                .header("content-type", "image/png")
                .body(buffer)
        }
        Err(_) => HttpResponse::NotFound().finish()
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new().service(
            web::resource("/{filename}.{extension}")
                .route(web::get().to(index))
        )

    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
