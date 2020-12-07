
use actix_web::{
    web, dev::BodyEncoding, get, http::ContentEncoding, middleware, App, HttpResponse, HttpServer,
};

// use serde::serde_derive::Deserialize;
use serde::{Serialize, Deserialize};


use image::io::Reader as ImageReader;
use image::DynamicImage;
use image::ImageError;
use image::ImageOutputFormat::{Png};

fn image_bytes(file: &FileInfo, conversion: &ConversionInfo) -> Result<Vec<u8>, ImageError> {

    let full_filename = format!("{}.{}", &file.name, &file.extension);

    let mut buffer: Vec<u8> = Vec::new();
    let img = ImageReader::open(full_filename)?.decode()?;
    img.write_to(&mut buffer, Png)?;

    Ok(buffer)
}

#[derive(Deserialize)]
struct FileInfo {
    name: String,
    extension: String,
}

#[derive(Deserialize)]
struct ConversionInfo {
    extension: Option<String>,
}

async fn index(
    file: web::Path<FileInfo>,
    conversion: web::Query<ConversionInfo>,
) -> HttpResponse {

    match image_bytes(&file, &conversion) {
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
            web::resource("/{name}.{extension}")
                .route(web::get().to(index))
        )

    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
