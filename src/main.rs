
use actix_web::{
    web,
    App,
    HttpResponse,
    HttpServer,
};

use image::io::Reader as ImageReader;
use image::ImageError;
use image::ImageOutputFormat;


// Return the bytes of a static png file after converting to webp format.
fn image_as_webp() -> Result<Vec<u8>, ImageError> {
    let full_filename = "rust.png";
    let mut buffer: Vec<u8> = Vec::new();

    let img = ImageReader::open(full_filename)?.with_guessed_format()?;
    let decoded = img.decode()?;

    let webp_encoder = webp::Encoder::from_image(&decoded);
    let webp = webp_encoder.encode_lossless();

    for i in 0..webp.len() {
        buffer.push(webp[i]);
    }

    Ok(buffer)
}

// Return the bytes of a static png file 
fn image_as_png() -> Result<Vec<u8>, ImageError> {
    let full_filename = "rust.png";
    let mut buffer: Vec<u8> = Vec::new();
    let img = ImageReader::open(full_filename)?.with_guessed_format()?;
    let decoded = img.decode()?;
    decoded.write_to(&mut buffer, ImageOutputFormat::Png)?;
    Ok(buffer)
}

fn png_handler() -> HttpResponse {
    let buffer = image_as_png().unwrap();
    HttpResponse::Ok()
        .header("content-type", "image/png")
        .body(buffer)
}

fn webp_handler() -> HttpResponse {
    let buffer = image_as_webp().unwrap();
    HttpResponse::Ok()
        .header("content-type", "image/webp")
        .body(buffer)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/rust.png", web::get().to(png_handler))
            .route("/rust.webp", web::get().to(webp_handler))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
