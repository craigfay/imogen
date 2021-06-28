
use image::io::Reader as ImageReader;
use image::ImageError;
use image::ImageOutputFormat;
use image::imageops::FilterType;
use serde::Deserialize;
use futures::{StreamExt, TryStreamExt};
use std::io;
use std::io::prelude::*;
use std::io::Write;
use std::fs::File;
use webp;
use actix_multipart::Multipart;
use actix_web::{
    web,
    App,
    HttpResponse,
    HttpServer,
    Error,
};


#[derive(Deserialize, Debug)]
struct ImageParams {
    width: u32,
    height: u32,
	ext: String,
}

type Bytes = Vec<u8>;
type ImageServiceResult = Result<Bytes, &'static str>;



// Respond to a request to upload a file contained in a multipart form stream
async fn upload(mut payload: Multipart) -> Result<HttpResponse, Error> {
    // Iterating over each part of the multipart form
    while let Ok(Some(mut field)) = payload.try_next().await {
        let content_type = field.content_disposition().unwrap();
        let filename = content_type.get_filename().unwrap();
        let filepath = format!("./uploads/{}", sanitize_filename::sanitize(&filename));

        // Creating a file on the host system. File::create is a blocking
        // operation, using a threadpool for this operation would improve
        // performance and scalability.
        let mut f = web::block(|| std::fs::File::create(filepath))
            .await
            .unwrap();

        while let Some(chunk) = field.next().await {
            let data = chunk.unwrap();

            // Writing bytes to the newly created file. Again, it would be
            // better to use a threadpool
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

// Given a set of image parameters, read an image from file, maybe apply
// transformations to it, and return its binary data.
fn image_bytes(params: ImageParams) -> ImageServiceResult {
    // Attempting to open a file
    let mut file = match File::open("rust.webp") {
        Err(_) => return Result::Err("A file with that name does not exist"),
        Ok(f) => f,
    };

    // Reading the contents of the file into a vector of bytes
    let mut buffer: Vec<u8> = Vec::new();
    file.read_to_end(&mut buffer);

    // Decoding the bytes as webp
    let webp_decoder = webp::Decoder::new(&buffer);
    let webp_image = webp_decoder.decode().unwrap();

    match &params.ext[..] {
        "webp" => {},
        "png" => {},
        "jpg" => {},
        _ => return Result::Err("Unsupported file format"),
    };

    // Re-encoding the bytes as png
    let dynamic_image = webp_image.to_image();

    // Resizing the image
    let resized_image = dynamic_image.resize_exact(
        params.width,
        params.height,
        FilterType::Nearest,
    );

    // Writing the resized image to a new byte vector
    let mut buffer: Vec<u8> = Vec::new();
    resized_image.write_to(&mut buffer, ImageOutputFormat::Png).unwrap();
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

fn dynamic_handler(params: web::Path<ImageParams>) -> HttpResponse {
    let buffer = image_bytes(params.into_inner()).unwrap();
    HttpResponse::Ok()
        .header("content-type", "image/png")
        .body(buffer)
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
            .route("/rust_{width}x{height}.{ext}", web::get().to(dynamic_handler))
            .route("/rust.png", web::get().to(png_handler))
            .route("/rust.webp", web::get().to(webp_handler))
            .route("/upload", web::post().to(upload))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
