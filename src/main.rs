
use actix_web::{
    web, dev::BodyEncoding, get, http::ContentEncoding, middleware, App, HttpResponse, HttpServer,
};

use serde::{Serialize, Deserialize};


use image::io::Reader as ImageReader;
use image::DynamicImage;
use image::ImageError;
use image::ImageOutputFormat;


fn image_bytes(file: &FileInfo, conversion: &ConversionOptions) -> Result<Vec<u8>, ImageError> {

    let full_filename = format!("{}.{}", &file.name, &file.extension);

    let mut buffer: Vec<u8> = Vec::new();

    let img = ImageReader::open(full_filename)?.with_guessed_format()?;

    let decoded = img.decode()?;

    let webp_encoder = webp::Encoder::from_image(&decoded);

    decoded.write_to(&mut buffer, conversion.output_format.clone())?;

    Ok(buffer)
}


#[derive(Deserialize)]
struct FileInfo {
    name: String,
    extension: String,
}

#[derive(Deserialize)]
struct RawConversionOptions {
    extension: Option<String>,
}

struct ConversionOptions {
    output_format: ImageOutputFormat,
    // TODO output_dimensions
}

fn extension_to_output_format(extension: &str) -> ImageOutputFormat {
    match extension {
        "jpeg" => ImageOutputFormat::Jpeg(255),
        "png" => ImageOutputFormat::Png,
        _ => ImageOutputFormat::Png,
    }
}

fn extension_from_output_format(output_format: &ImageOutputFormat) -> String {
    match output_format {
        ImageOutputFormat::Jpeg(255) => "jpeg",
        ImageOutputFormat::Png => "png",
        _ => "png",
    }.to_string()
}

fn validate_conversion(
    file: &FileInfo,
    raw: &RawConversionOptions,
) -> ConversionOptions {

    let output_format = match &raw.extension {
        Some(e) => extension_to_output_format(&e),
        None => extension_to_output_format(&file.extension),
    };

    ConversionOptions {
        output_format
    }

}

async fn index(
    file: web::Path<FileInfo>,
    raw_conversion_options: web::Query<RawConversionOptions>,
) -> HttpResponse {

    let conversion_options = validate_conversion(&file, &raw_conversion_options);

    let content_type = format!(
        "image/{}",
        extension_from_output_format(&conversion_options.output_format)
    );

    match image_bytes(&file, &conversion_options) {
        Ok(buffer) => {
            HttpResponse::Ok()
                .header("content-type", content_type)
                .body(buffer)
        },
        Err(e) => {
            println!("{:?}", e);
            HttpResponse::NotFound().finish()
        }
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
