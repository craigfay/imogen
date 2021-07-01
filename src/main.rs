
use image::io::Reader as ImageReader;
use image::ImageError;
use image::ImageOutputFormat;
use image::imageops::FilterType;
use image::GenericImageView;
use serde::{Serialize, Deserialize};
use futures::{StreamExt, TryStreamExt};
use std::io;
use std::io::Cursor;
use std::io::prelude::*;
use std::io::Write;
use std::fs::File;
use webp;
use actix_multipart::Multipart;
use actix_web::{
    web,
    App,
    HttpResponse,
    HttpRequest,
    HttpServer,
    Error,
};

enum ImageServiceFailure {
    UnsupportedFormat,
    ImageDoesNotExist,
}

impl ImageServiceFailure {
    fn to_string(&self) -> String {
        match self {
            Self::UnsupportedFormat => "Unsupported file format".to_string(),
            Self::ImageDoesNotExist => "Requested image does not exist".to_string(),
        }
    }
}

type Bytes = Vec<u8>;
type ImageServiceResult = Result<Bytes, ImageServiceFailure>;


// "example.png" -> "example"
fn strip_extension(filename: &str) -> String {
    let mut parts: Vec<&str> = filename.split(".").collect();
    parts.pop();
    parts.join(".")
}

#[derive(Serialize)]
struct UploadResult {
    pub filename: String,
    pub errors: Vec<String>,
}

impl UploadResult {
    pub fn with_error(mut self, message: &str) -> Self {
        self.errors.push(message.to_string());
        self
    }
}

// Respond to a request to upload a file contained in a multipart form stream
async fn upload(mut payload: Multipart) -> Result<HttpResponse, Error> {

    let mut results: Vec<UploadResult> = vec![];

    // Iterating over each part of the multipart form
    'form_parts: while let Ok(Some(mut field)) = payload.try_next().await {
        let content_type = field.content_disposition().unwrap();
        let filename = content_type.get_filename().unwrap();
        let filename = strip_extension(&filename);
        let filepath = format!("./uploads/{}.webp", filename);

        let mut result = UploadResult { filename, errors: vec![] };

        // Reading file data
        let mut incoming_data: Vec<u8> = Vec::new();
        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(data) => incoming_data.extend(data),
                Err(_) => {
                    results.push(result.with_error("File failed to re-assemble"));
                    continue 'form_parts;
                }
            };
        }

        let cursor = Cursor::new(incoming_data);
        let mut reader = match ImageReader::new(cursor).with_guessed_format() {
            Ok(result) => result, 
            Err(_) => {
                results.push(result.with_error("File was un-readable"));
                continue 'form_parts;
            }
        };

        let dynamic_image = match reader.decode() {
            Ok(result) => result,
            Err(_) => {
                results.push(result.with_error("File could not be decoded"));
                continue 'form_parts;
            }
        };

        // Re-encoding as Webp
        let mut data_to_store: Vec<u8> = Vec::new();
        let webp_encoder = webp::Encoder::from_image(&dynamic_image);
        let webp = webp_encoder.encode_lossless();
        for i in 0..webp.len() { data_to_store.push(webp[i]); }

        // Creating a file on the host system. File::create is a blocking
        // operation, using a threadpool for this operation would improve
        // performance and scalability.

        let mut f = match web::block(|| File::create(filepath)).await {
            Ok(result) => result,
            Err(_) => {
                results.push(result.with_error("New file could not be created"));
                continue 'form_parts;
            }
        };

        f = match web::block(move || f.write_all(&data_to_store).map(|_| f)).await {
            Ok(result) => result,
            Err(_) => {
                results.push(result.with_error("File contents could not be saved"));
                continue 'form_parts;
            }
        };

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
fn image_bytes(required: &RequiredImageParams, optional: &OptionalImageParams) -> ImageServiceResult {
    // Attempting to open a file
    let filepath = format!("./uploads/{}.webp", required.filename);
    let mut file = match File::open(filepath) {
        Err(_) => return Result::Err(ImageServiceFailure::ImageDoesNotExist),
        Ok(f) => f,
    };

    // Reading the contents of the file into a vector of bytes
    let mut buffer: Vec<u8> = Vec::new();
    file.read_to_end(&mut buffer);

    // Decoding the bytes as webp
    let webp_decoder = webp::Decoder::new(&buffer);
    let webp_image = webp_decoder.decode().unwrap();
    let mut dynamic_image = webp_image.to_image();

    // Choosing resize dimensions
    let width = dynamic_image.width();
    let height = dynamic_image.height();
    let new_width = optional.w.unwrap_or(width);
    let new_height = optional.h.unwrap_or(height);

    // Choosing sampling method filter to use for resizing
    let filter = match &optional.sampling {
        None => FilterType::Nearest,
        Some(filter_name) => match &filter_name[..] {
            "triangle" => FilterType::Triangle,
            "catmullrom" => FilterType::CatmullRom,
            "gaussian" => FilterType::Gaussian,
            "lanczos3" => FilterType::Lanczos3,
            _ => FilterType::Nearest,
        }
    };

    // Resizing the image
    if height != new_height || width != new_width {
        dynamic_image = match &optional.stretch.unwrap_or(false) {
            true => dynamic_image.resize_exact(new_width, new_height, filter),
            false => dynamic_image.resize(new_width, new_height, filter),
        }
    }

    // Initializing the output bytes
    let mut buffer: Vec<u8> = Vec::new();

    // Re-encoding the image and writing to the buffer
    match &required.extension[..] {
        "webp" => {
            let webp_encoder = webp::Encoder::from_image(&dynamic_image);
            let webp = webp_encoder.encode_lossless();
            for i in 0..webp.len() { buffer.push(webp[i]); }
            Ok(buffer)
        },
        "png" => {
            dynamic_image.write_to(&mut buffer, ImageOutputFormat::Png).unwrap();
            Ok(buffer)
        },
        "jpeg" => {
            dynamic_image.write_to(&mut buffer, ImageOutputFormat::Jpeg(255)).unwrap();
            Ok(buffer)
        },
        _ => Result::Err(ImageServiceFailure::UnsupportedFormat)
    }
}

#[derive(Deserialize, Debug)]
struct RequiredImageParams {
    filename: String,
    extension: String,
}

#[derive(Deserialize, Debug)]
struct OptionalImageParams {
    stretch: Option<bool>,
    sampling: Option<String>,
    w: Option<u32>,
    h: Option<u32>,
}

fn serve_image_via_http(required: web::Path<RequiredImageParams>, optional: web::Query<OptionalImageParams>) -> HttpResponse {
    let required = required.into_inner();
    let optional = optional.into_inner();

    match image_bytes(&required, &optional) {
        Ok(buffer) => {
            HttpResponse::Ok()
                .header("content-type", format!("image/{}", required.extension))
                .body(buffer)
        },
        Err(failure) => match failure {
            ImageServiceFailure::ImageDoesNotExist => {
                HttpResponse::NotFound().body(failure.to_string())
            }
            ImageServiceFailure::UnsupportedFormat => {
                HttpResponse::BadRequest().body(failure.to_string())
            }
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .route("/{filename}.{extension}", web::get().to(serve_image_via_http))
            .route("/upload", web::post().to(upload))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
