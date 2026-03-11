use std::{
    env,
    error::Error,
    ffi::OsStr,
    path::{Path, PathBuf},
};

use reqwest::multipart::{Form, Part};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct UploadMediaResponse {
    id: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let _ = dotenvy::dotenv();

    let file_path = parse_file_path()?;
    let phone_id = read_required_env("WHATSAPP_PHONE_ID")?;
    let token = read_required_env("WHATSAPP_TOKEN")?;

    let media_id = upload_media(&token, &phone_id, &file_path).await?;
    println!("{media_id}");

    Ok(())
}

fn parse_file_path() -> Result<PathBuf, Box<dyn Error>> {
    let mut args = env::args_os();
    let _program = args.next();

    let Some(path) = args.next() else {
        return Err("uso: cargo run --bin upload_media -- <ruta_del_archivo>".into());
    };

    if args.next().is_some() {
        return Err("solo se permite una ruta de archivo por ejecucion".into());
    }

    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(format!("archivo no encontrado: {}", path.display()).into());
    }

    Ok(path)
}

fn read_required_env(name: &'static str) -> Result<String, Box<dyn Error>> {
    env::var(name).map_err(|_| format!("falta la variable de entorno {name}").into())
}

async fn upload_media(
    token: &str,
    phone_id: &str,
    file_path: &Path,
) -> Result<String, Box<dyn Error>> {
    let bytes = tokio::fs::read(file_path).await?;
    let file_name = file_path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or("nombre de archivo invalido")?;
    let mime = detect_mime(file_path);

    let file_part = Part::bytes(bytes)
        .file_name(file_name.to_string())
        .mime_str(mime)?;

    let form = Form::new()
        .text("messaging_product", "whatsapp")
        .part("file", file_part);

    let url = format!("https://graph.facebook.com/v21.0/{phone_id}/media");
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(format!("error al subir media a Meta ({status}): {body}").into());
    }

    let payload: UploadMediaResponse = serde_json::from_str(&body)?;
    Ok(payload.id)
}

fn detect_mime(path: &Path) -> &'static str {
    match path.extension().and_then(OsStr::to_str) {
        Some(ext) if ext.eq_ignore_ascii_case("png") => "image/png",
        Some(ext) if ext.eq_ignore_ascii_case("jpg") => "image/jpeg",
        Some(ext) if ext.eq_ignore_ascii_case("jpeg") => "image/jpeg",
        Some(ext) if ext.eq_ignore_ascii_case("webp") => "image/webp",
        Some(ext) if ext.eq_ignore_ascii_case("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}
