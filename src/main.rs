
use actix_web::{
    web, dev::BodyEncoding, get, http::ContentEncoding, middleware, App, HttpResponse, HttpServer,
};

use image::io::Reader as ImageReader;
use image::DynamicImage;
use image::ImageError;
use image::ImageOutputFormat::{Png};

fn image_bytes() -> Result<Vec<u8>, ImageError> {
    let mut buffer: Vec<u8> = Vec::new();
    let img = ImageReader::open("rust.png")?.decode()?;
    img.write_to(&mut buffer, Png)?;

    Ok(buffer)
}

async fn index() -> HttpResponse {
    match image_bytes() {
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
