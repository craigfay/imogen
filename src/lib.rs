use std::fs::File;
use std::path::Path;
use std::io::ErrorKind as IOError;
use image::io::Reader as ImageReader;
use image::imageops::FilterType;
use image::{
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
use actix_files::NamedFile;
use actix_web::{
    web,
    App,
    HttpRequest,
    HttpResponse,
    HttpServer,
    Error,
};


enum ImageServiceFailure {
    UnsupportedFormat,
    ImageDoesNotExist,
    MemoryOverflow,
    CouldNotReadToBuffer,
}

impl ImageServiceFailure {
    fn to_string(&self) -> String {
        match self {
            Self::UnsupportedFormat => "Unsupported file format".to_string(),
            Self::ImageDoesNotExist => "Requested image does not exist".to_string(),
            Self::MemoryOverflow => "Failed to allocate adequate memory".to_string(),
            Self::CouldNotReadToBuffer => "Could not load image into memory buffer".to_string(),
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
async fn upload(mut payload: Multipart, config: web::Data<ServerConfig>) -> Result<HttpResponse, Error> {
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

        // Determining upload path
        let filename = filename.to_string();
        let clean_filename = strip_extension(&filename);
        let filepath = format!("{}/{}.webp", config.uploads_dir, clean_filename);
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

fn try_loading_unprocessed_image(filepath: &str) -> ImageServiceResult {
    let mut file = match File::open(filepath) {
        Err(_) => return Err(ImageServiceFailure::ImageDoesNotExist),
        Ok(f) => f,
    };

    // Reading the contents of the file into a vector of bytes
    let mut buffer: Bytes = Vec::new();
    match file.read_to_end(&mut buffer) {
        Ok(_) => {},
        Err(io_err) => return Err( match io_err.kind() {
            IOError::OutOfMemory => ImageServiceFailure::MemoryOverflow,
            _ => ImageServiceFailure::CouldNotReadToBuffer,
        }),
    };

    Ok(buffer)
}


fn try_processing_image(
    buffer: Bytes,
    optional: &ProcessingInstructions,
    required: &FileDescription,
) -> ImageServiceResult {
    // Decoding bytes as webp
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
struct FileDescription {
    filename: String,
    extension: String,
}

#[derive(Deserialize, Debug)]
struct ProcessingInstructions {
    stretch: Option<bool>,
    sampling: Option<String>,
    w: Option<u32>,
    h: Option<u32>,
}

fn potentially_streamable_file(path: &str) -> Option<NamedFile> {
    match NamedFile::open(path) {
        Ok(file) => Some(file),
        Err(_) => None,
    }
}

fn try_streaming_preprocessed_file_from_disk(
    filepath: &str,
    req: &HttpRequest,
) -> Option<HttpResponse> {
    match potentially_streamable_file(&filepath) {
        None => None,
        Some(file) => match file.into_response(&req) {
            Ok(response) => Some(response),
            Err(_) => None,
        }
    }
}

fn path_to_requested_file_if_exists(
    req: & HttpRequest,
    config: &web::Data<ServerConfig>
) -> String {
    format!("{}/{}?{}", config.uploads_dir, req.path(), req.query_string())
}


impl ImageServiceFailure {
    fn as_http_response(&self) -> HttpResponse {
        match self {
            ImageServiceFailure::ImageDoesNotExist => {
                HttpResponse::NotFound().body(self.to_string())
            }
            ImageServiceFailure::UnsupportedFormat => {
                HttpResponse::BadRequest().body(self.to_string())
            }
            ImageServiceFailure::MemoryOverflow => {
                HttpResponse::InternalServerError().body(self.to_string())
            }
            ImageServiceFailure::CouldNotReadToBuffer => {
                HttpResponse::InternalServerError().body(self.to_string())
            }
        }
    }
}

fn image_buffer_as_http_response(buffer: Bytes, extension: &str) -> HttpResponse {
    HttpResponse::Ok()
        .header("content-type", format!("image/{}", extension))
        .body(buffer)
}



fn build_processing_suffix(req: &HttpRequest) -> String {
    let qs = req.query_string();
    match qs.len() {
        0 => "".to_string(),
        _ => format!("?{}", qs),
    }
}

fn build_path_to_preprocessed_file(
    file_desc: &FileDescription,
    processing_suffix: &String,
    config: &ServerConfig,
) -> String {
    format!(
        "{}/{}{}.{}",
        config.uploads_dir,
        file_desc.filename,
        processing_suffix,
        file_desc.extension,
    )
}

fn build_path_to_unprocessed_file(
    file_desc: &FileDescription,
    config: &ServerConfig,
) -> String {
    format!(
        "{}/{}.{}",
        config.uploads_dir,
        file_desc.filename,
        file_desc.extension,
    )
}

struct ImageRequest {
    req: HttpRequest,
    filepath_if_preprocessed: String,
    filepath_if_unprocessed: String,
    processing: ProcessingInstructions,
}

impl ImageRequest {
    fn build(
        req: HttpRequest,
        file_desc: web::Path<FileDescription>,
        processing: web::Query<ProcessingInstructions>,
        config: web::Data<ServerConfig>,
    ) -> Self {
        let processing = processing.into_inner();
        let file_desc = file_desc.into_inner();
        let processing_suffix = build_processing_suffix(&req);

        Self {
            req,
            processing,
            filepath_if_preprocessed: build_path_to_preprocessed_file(
                &file_desc,
                &processing_suffix,
                &config,
            ),
            filepath_if_unprocessed: build_path_to_unprocessed_file(
                &file_desc,
                &config,
            ),
        }
    }
}

fn serve_image_via_http(
    req: HttpRequest,
    required: web::Path<FileDescription>,
    optional: web::Query<ProcessingInstructions>,
    config: web::Data<ServerConfig>,
) -> HttpResponse {
    let required = required.into_inner();
    let optional = optional.into_inner();

    let preprocessed_filename = match req.query_string() != "" || required.extension != "webp" {
        true => format!("{}/{}?{}.{}",config.uploads_dir, required.filename, req.query_string(), required.extension),
        false => format!("{}/{}.{}",config.uploads_dir, required.filename, required.extension),
    };

    match try_streaming_preprocessed_file_from_disk(&preprocessed_filename, &req) {
        Some(response) => return response,
        None => {},
    };

    let unprocessed_filename = format!("{}/{}.webp", config.uploads_dir, required.filename);

    let unprocessed_image = match try_loading_unprocessed_image(&unprocessed_filename) {
        Err(failure) => return failure.as_http_response(),
        Ok(bytes) => bytes,
    };

    let processed_image = match try_processing_image(unprocessed_image, &optional, &required) {
        Err(failure) => return failure.as_http_response(),
        Ok(buffer) => buffer,
    };

    let mut file = File::create(preprocessed_filename).unwrap();
    file.write_all(&processed_image);
    
    image_buffer_as_http_response(processed_image, &required.extension)
}

struct ServerConfig {
    uploads_dir: String,
}


pub struct ImageServer;

impl ImageServer {
    pub fn listen(port: u64, uploads_dir: String) {
        let config = web::Data::new(ServerConfig { uploads_dir });

        // Creating uploads directory if non-existent
        std::fs::create_dir_all(Path::new(&config.uploads_dir))
            .expect("Unable to create uploads directory");

        let serve_forever = async move {
            HttpServer::new(move || {
                App::new()
                    .app_data(config.clone())
                    // .service(Files::new("/", "./uploads").prefer_utf8(true))
                    .route("/{filename}.{extension}", web::get().to(serve_image_via_http))
                    .route("/upload", web::post().to(upload))
            })
            .bind(format!("0.0.0.0:{}", port))
            .expect(&format!("Failed to bind to port {}", port))
            .run()
            .await
        };

        actix_web::rt::System::new("server")
            .block_on(serve_forever)
            .expect("Failed to create async runtime")
    }
}

