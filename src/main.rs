use std::fs::File;
use std::path::Path;
use image::io::Reader as ImageReader;
use image::imageops::FilterType;
use image::{
    ImageError,
    ImageOutputFormat,
    GenericImageView,
    ImageFormat,
};
use webp;
use serde::{Serialize, Deserialize};
use serde_json;
use futures::{StreamExt, TryStreamExt};
use std::io::{
    Cursor,
    Write,
    Read,
};
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
    pub filename: Option<String>,
    pub errors: Vec<String>,
}

impl UploadResult {
    pub fn new() -> Self {
        Self { filename: None, errors: vec![] }
    }

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
        let mut result = UploadResult::new();

        let content_type = match field.content_disposition() {
            Some(result) => result,
            None => {
                let message = "The multi-part form was improperly formatted: \
                Content-Disposition was not present";
                results.push(result.with_error(message));
                continue 'form_parts;
            }
        };

        // Determining filename
        let filename = match content_type.get_filename() {
            Some(filename) => filename,
            None => {
                let message = "The multi-part form was improperly formatted: \
                A filename was not provided";
                results.push(result.with_error(message));
                continue 'form_parts;
            }
        };

        let filename = filename.to_string();
        let clean_filename = strip_extension(&filename);
        let filepath = format!("./uploads/{}.webp", clean_filename);
        if filename != "" { result.filename = Some(filename); }

        // Preventing duplicate filenames
        if Path::new(&filepath).exists() {
            let message = "Another file with this name already exists.";
            results.push(result.with_error(message));
            continue 'form_parts;
        }

        // Reading file data
        let mut incoming_data: Bytes = Vec::new();
        while let Some(chunk) = field.next().await {
            match chunk {
                Ok(data) => incoming_data.extend(data),
                Err(_) => {
                    let message = "File failed to re-assemble.";
                    results.push(result.with_error(message));
                    continue 'form_parts;
                }
            };
        }

        // Preventing empty file uploads
        if incoming_data.len() == 0 {
            results.push(result.with_error("No file data was provided."));
            continue 'form_parts;
        }

        // Constructing Image Reader
        let cursor = Cursor::new(incoming_data);
        let reader = match ImageReader::new(cursor).with_guessed_format() {
            Ok(result) => result, 
            Err(_) => {
                results.push(result.with_error("File was un-readable."));
                continue 'form_parts;
            }
        };

        // Restricting file formats
        match reader.format() {
            Some(ImageFormat::Png) => {},
            Some(ImageFormat::Jpeg) => {},
            Some(ImageFormat::WebP) => {},
            _ => {
                let message = "Unsupported file format. Try converting to \
                .png, .jpeg, or .webp before uploading.";
                results.push(result.with_error(message));
                continue 'form_parts;
            }
        }

        // Decoding image data
        let dynamic_image = match reader.decode() {
            Ok(result) => result,
            Err(_) => {
                let message = "File data could not be decoded.";
                results.push(result.with_error(message));
                continue 'form_parts;
            }
        };

        // Re-encoding uploaded image as WebP
        let mut data_to_store: Bytes = Vec::new();
        let webp_encoder = webp::Encoder::from_image(&dynamic_image);
        let webp = webp_encoder.encode_lossless();
        for i in 0..webp.len() { data_to_store.push(webp[i]); }

        // Creating new file on a new threadpool
        let mut f = match web::block(|| File::create(filepath)).await {
            Ok(result) => result,
            Err(_) => {
                let message = "New file could not be created.";
                results.push(result.with_error(message));
                continue 'form_parts;
            }
        };

        // Writing contents to file on a new threadpool
        match web::block(move || f.write_all(&data_to_store).map(|_| f)).await {
            Ok(result) => result,
            Err(_) => {
                let message = "File contents could not be saved";
                results.push(result.with_error(message));
                continue 'form_parts;
            }
        };

        // Success!
        results.push(result);
    }

    Ok(
        HttpResponse::Ok()
            .header("content-type", "application/json")
            .body(serde_json::to_string(&results).unwrap())
    )
}


// Return the bytes of a static png file after converting to webp format.
fn image_as_webp() -> Result<Bytes, ImageError> {
    let full_filename = "rust.png";
    let mut buffer: Bytes = Vec::new();

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
    let mut buffer: Bytes = Vec::new();
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
    let mut buffer: Bytes = Vec::new();

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



struct ImageServer {
    upload_to_dir: String,
    upload_url: String,
    port: Option<u64>,
}

impl ImageServer {
    pub async fn listen(&mut self, port: u64) -> std::io::Result<()> {
        self.port = Some(port);
        HttpServer::new(|| {
            App::new()
                .route("/{filename}.{extension}", web::get().to(serve_image_via_http))
                .route("/upload", web::post().to(upload))
        })
        .bind(format!("127.0.0.1:{}", port))
        .unwrap()
        .run()
        .await
    }
}

fn main() {
    let mut server = ImageServer {
        upload_to_dir: "./uploads".to_string(),
        upload_url: "/uploads".to_string(),
        port: None,
    };

    actix_web::rt::System::new("server")
        .block_on(async move {
            server.listen(8080).await;
        });
}
