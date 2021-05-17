
use actix_web::{
    web,
    App,
    HttpResponse,
    HttpServer,
    Error,
};

use actix_multipart::Multipart;

use futures::{StreamExt, TryStreamExt};
use std::io::Write;


use image::io::Reader as ImageReader;
use image::ImageError;
use image::ImageOutputFormat;

async fn upload(mut payload: Multipart) -> Result<HttpResponse, Error> {
    // iterate over multipart stream
    while let Ok(Some(mut field)) = payload.try_next().await {
        let content_type = field.content_disposition().unwrap();
        let filename = content_type.get_filename().unwrap();
        let filepath = format!("./uploads/{}", sanitize_filename::sanitize(&filename));

        // File::create is blocking operation, use threadpool
        let mut f = web::block(|| std::fs::File::create(filepath))
            .await
            .unwrap();

        // Field in turn is stream of *Bytes* object
        while let Some(chunk) = field.next().await {
            let data = chunk.unwrap();
            // filesystem operations are blocking, we have to use threadpool
            f = web::block(move || f.write_all(&data).map(|_| f)).await?;
        }
    }
    Ok(HttpResponse::Ok().into())
}


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
            .route("/upload", web::post().to(upload))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
