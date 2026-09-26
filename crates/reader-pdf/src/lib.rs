//! Native PDF operations are confined to a single, lazily started worker thread.
//! Public document and selection data contain no PDFium handles.

use futures_channel::oneshot;
use pdfium_render::prelude::{
    PdfDocument, PdfDocumentMetadataTagType, PdfPage, PdfRenderConfig, Pdfium, PdfiumError,
    PdfiumInternalError,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, mpsc};

const MAX_SOURCE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_PAGES: usize = 100_000;
const MAX_GLYPHS: i32 = 200_000;
const MAX_COPY_BYTES: usize = 16 * 1024 * 1024;
const MAX_PIXELS: f64 = 4_000_000.0;
const MAX_DIMENSION: f64 = 8192.0;

#[derive(Clone, Copy, Debug)]
pub struct PageInfo {
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

#[derive(Clone, Debug)]
pub struct Glyph {
    pub start: usize,
    pub end: usize,
    pub bounds: Option<Rect>,
}

#[derive(Clone, Debug)]
pub struct TextLayer {
    pub text: String,
    pub glyphs: Vec<Glyph>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct TextPoint {
    pub page: u32,
    pub index: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Selection {
    pub anchor: TextPoint,
    pub focus: TextPoint,
}

#[derive(Debug)]
pub struct RenderedPage {
    pub page: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub text: Arc<TextLayer>,
}

#[derive(Debug)]
pub struct Document {
    pub id: u64,
    pub path: PathBuf,
    pub title: String,
    pub fingerprint: String,
    pub pages: Vec<PageInfo>,
    pub can_copy: bool,
    pub session: Session,
}

#[derive(Clone)]
pub struct Session(Arc<SessionInner>);

struct SessionInner {
    id: u64,
    worker: mpsc::Sender<Command>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("id", &self.0.id).finish()
    }
}

impl Drop for SessionInner {
    fn drop(&mut self) {
        // A queued close runs after any request that was already submitted.
        let _ = self.worker.send(Command::Close(self.id));
    }
}

struct OpenData {
    id: u64,
    path: PathBuf,
    title: String,
    fingerprint: String,
    pages: Vec<PageInfo>,
    can_copy: bool,
}

enum Command {
    Open(u64, PathBuf, oneshot::Sender<Result<OpenData, String>>),
    Render(u64, u32, u32, oneshot::Sender<Result<RenderedPage, String>>),
    Copy(
        u64,
        Option<Selection>,
        oneshot::Sender<Result<String, String>>,
    ),
    Close(u64),
}

static WORKER: LazyLock<Result<mpsc::Sender<Command>, String>> = LazyLock::new(|| {
    let (sender, receiver) = mpsc::channel();
    std::thread::Builder::new()
        .name("pdfium-reader".into())
        .spawn(move || run_worker(receiver))
        .map_err(|error| format!("Cannot start PDF reader worker: {error}"))?;
    Ok(sender)
});
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn worker() -> Result<mpsc::Sender<Command>, String> {
    (*WORKER).clone()
}

/// Opens a file-backed PDF without replacing any previously opened document on failure.
pub async fn open(path: PathBuf) -> Result<Arc<Document>, String> {
    let sender = worker()?;
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    // Own the close guard before awaiting the reply. Cancellation after the
    // worker sends metadata must still release its native document.
    let session = Session(Arc::new(SessionInner {
        id,
        worker: sender.clone(),
    }));
    let (reply, result) = oneshot::channel();
    sender
        .send(Command::Open(id, path, reply))
        .map_err(|_| "PDF reader worker stopped".to_string())?;
    let data = result
        .await
        .map_err(|_| "PDF reader worker stopped".to_string())??;
    Ok(Arc::new(Document {
        id: data.id,
        path: data.path,
        title: data.title,
        fingerprint: data.fingerprint,
        pages: data.pages,
        can_copy: data.can_copy,
        session,
    }))
}

impl Session {
    pub async fn render(&self, page: u32, target_width: u32) -> Result<RenderedPage, String> {
        let (reply, result) = oneshot::channel();
        self.0
            .worker
            .send(Command::Render(self.0.id, page, target_width, reply))
            .map_err(|_| "PDF reader worker stopped".to_string())?;
        result
            .await
            .map_err(|_| "PDF reader worker stopped".to_string())?
    }

    pub async fn copy(&self, selection: Selection) -> Result<String, String> {
        self.copy_impl(Some(selection)).await
    }

    pub async fn copy_all(&self) -> Result<String, String> {
        self.copy_impl(None).await
    }

    async fn copy_impl(&self, selection: Option<Selection>) -> Result<String, String> {
        let (reply, result) = oneshot::channel();
        self.0
            .worker
            .send(Command::Copy(self.0.id, selection, reply))
            .map_err(|_| "PDF reader worker stopped".to_string())?;
        result
            .await
            .map_err(|_| "PDF reader worker stopped".to_string())?
    }
}

fn run_worker(receiver: mpsc::Receiver<Command>) {
    let pdfium = (|| {
        let executable = std::env::current_exe()
            .map_err(|error| format!("Cannot locate application executable: {error}"))?;
        let library = executable
            .parent()
            .ok_or("Application executable has no directory")?
            .join("pdfium.dll");
        let bindings = Pdfium::bind_to_library(&library).map_err(|error| {
            format!(
                "Cannot load bundled PDFium at {}: {error}",
                library.display()
            )
        })?;
        Ok::<_, String>(Pdfium::new(bindings))
    })();

    match pdfium {
        Ok(pdfium) => serve(&pdfium, receiver),
        Err(error) => {
            for command in receiver {
                match command {
                    Command::Open(_, _, reply) => {
                        let _ = reply.send(Err(error.clone()));
                    }
                    Command::Render(_, _, _, reply) => {
                        let _ = reply.send(Err(error.clone()));
                    }
                    Command::Copy(_, _, reply) => {
                        let _ = reply.send(Err(error.clone()));
                    }
                    Command::Close(_) => {}
                }
            }
        }
    }
}

fn serve<'a>(pdfium: &'a Pdfium, receiver: mpsc::Receiver<Command>) {
    let mut documents: HashMap<u64, PdfDocument<'a>> = HashMap::new();
    for command in receiver {
        match command {
            Command::Open(id, path, reply) => {
                if reply.is_canceled() {
                    continue;
                }
                match open_on_worker(pdfium, id, path, &reply) {
                    Ok((data, document)) => {
                        let id = data.id;
                        if reply.send(Ok(data)).is_ok() {
                            documents.insert(id, document);
                        }
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                    }
                }
            }
            Command::Render(id, page, target_width, reply) => {
                if reply.is_canceled() {
                    continue;
                }
                let result = documents
                    .get(&id)
                    .ok_or_else(|| "PDF document is closed".to_string())
                    .and_then(|document| render_page(document, page, target_width));
                let _ = reply.send(result);
            }
            Command::Copy(id, selection, reply) => {
                if reply.is_canceled() {
                    continue;
                }
                let result = documents
                    .get(&id)
                    .ok_or_else(|| "PDF document is closed".to_string())
                    .and_then(|document| copy_text(document, selection, &reply));
                let _ = reply.send(result);
            }
            Command::Close(id) => {
                documents.remove(&id);
            }
        }
    }
}

fn open_on_worker<'a>(
    pdfium: &'a Pdfium,
    id: u64,
    path: PathBuf,
    reply: &oneshot::Sender<Result<OpenData, String>>,
) -> Result<(OpenData, PdfDocument<'a>), String> {
    let path = path
        .canonicalize()
        .map_err(|error| format!("Cannot find PDF {}: {error}", path.display()))?;
    let mut file = File::open(&path)
        .map_err(|error| format!("Cannot open PDF {}: {error}", path.display()))?;
    let length = file
        .metadata()
        .map_err(|error| format!("Cannot read PDF size: {error}"))?
        .len();
    if length == 0 {
        return Err("PDF file is empty".into());
    }
    if length > MAX_SOURCE_BYTES {
        return Err("PDF source exceeds 512 MiB limit".into());
    }
    let fingerprint = hash_file(&mut file, length, reply)?;
    if reply.is_canceled() {
        return Err("PDF open was canceled".into());
    }
    file.rewind()
        .map_err(|error| format!("Cannot rewind PDF file: {error}"))?;
    let document = pdfium
        .load_pdf_from_reader(file, None)
        .map_err(pdf_load_error)?;
    let count = usize::try_from(document.pages().len())
        .map_err(|_| "PDF page count is invalid".to_string())?;
    if count == 0 {
        return Err("PDF has no pages".into());
    }
    if count > MAX_PAGES {
        return Err("PDF exceeds 100,000 page limit".into());
    }
    let mut pages = Vec::with_capacity(count);
    for index in 0..count {
        let size = document
            .pages()
            .page_size(index as i32)
            .map_err(|error| format!("Cannot inspect PDF page {}: {error}", index + 1))?;
        let width = size.width().value;
        let height = size.height().value;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(format!("PDF page {} has invalid dimensions", index + 1));
        }
        pages.push(PageInfo { width, height });
        if reply.is_canceled() {
            return Err("PDF open was canceled".into());
        }
    }
    let can_copy = document
        .permissions()
        .can_extract_text_and_graphics()
        .map_err(|error| format!("Cannot inspect PDF copy permissions: {error}"))?;
    let title = document
        .metadata()
        .get(PdfDocumentMetadataTagType::Title)
        .map(|metadata| metadata.value().trim().to_string())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| {
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
    Ok((
        OpenData {
            id,
            path,
            title,
            fingerprint,
            pages,
            can_copy,
        },
        document,
    ))
}

fn hash_file(
    file: &mut File,
    expected_size: u64,
    reply: &oneshot::Sender<Result<OpenData, String>>,
) -> Result<String, String> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        if reply.is_canceled() {
            return Err("PDF open was canceled".into());
        }
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Cannot hash PDF source: {error}"))?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > MAX_SOURCE_BYTES {
            return Err("PDF source exceeds 512 MiB limit".into());
        }
        hash.update(&buffer[..read]);
    }
    if size != expected_size {
        return Err("PDF source changed during opening".into());
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn pdf_load_error(error: PdfiumError) -> String {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            "Password-protected PDF cannot be opened".into()
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::SecurityError) => {
            "PDF security settings are unsupported".into()
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            "Malformed or unsupported PDF format".into()
        }
        _ => format!("Cannot parse PDF: {error}"),
    }
}

fn page_at<'a>(document: &PdfDocument<'a>, page: u32) -> Result<PdfPage<'a>, String> {
    let number = u64::from(page) + 1;
    let index = i32::try_from(page).map_err(|_| format!("PDF page {number} is out of range"))?;
    if index >= document.pages().len() {
        return Err(format!("PDF page {number} is out of range"));
    }
    document
        .pages()
        .get(index)
        .map_err(|error| format!("Cannot load PDF page {number}: {error}"))
}

fn raster_size(page: &PdfPage<'_>, target_width: u32) -> Result<(u32, u32), String> {
    if target_width == 0 {
        return Err("PDF raster width must be positive".into());
    }
    let width = f64::from(page.width().value);
    let height = f64::from(page.height().value);
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err("PDF page has invalid dimensions".into());
    }
    let requested_width = f64::from(target_width);
    let requested_height = requested_width * height / width;
    if !requested_height.is_finite() || requested_height <= 0.0 {
        return Err("PDF page aspect ratio cannot be rendered".into());
    }
    let scale = 1.0_f64
        .min(MAX_DIMENSION / requested_width)
        .min(MAX_DIMENSION / requested_height)
        .min((MAX_PIXELS / (requested_width * requested_height)).sqrt());
    let raster_width = (requested_width * scale).floor() as u32;
    let raster_height = (requested_height * scale).floor() as u32;
    if raster_width == 0 || raster_height == 0 {
        return Err("PDF page aspect ratio exceeds raster limits".into());
    }
    Ok((raster_width, raster_height))
}

fn render_page(
    document: &PdfDocument<'_>,
    page_index: u32,
    target_width: u32,
) -> Result<RenderedPage, String> {
    let page = page_at(document, page_index)?;
    let (width, height) = raster_size(&page, target_width)?;
    let config = PdfRenderConfig::new()
        .set_target_size(width as i32, height as i32)
        .render_annotations(false)
        .render_form_data(false)
        .limit_render_image_cache_size(true);
    let bitmap = page
        .render_with_config(&config)
        .map_err(|error| format!("Cannot render PDF page {}: {error}", page_index + 1))?;
    let actual_width = bitmap.width() as u32;
    let actual_height = bitmap.height() as u32;
    if actual_width != width || actual_height != height {
        return Err("PDF raster dimensions differ from requested size".into());
    }
    let rgba = bitmap.as_rgba_bytes();
    if rgba.len() != width as usize * height as usize * 4 {
        return Err("PDF raster has invalid RGBA byte count".into());
    }
    drop(bitmap);
    let permitted = document
        .permissions()
        .can_extract_text_and_graphics()
        .map_err(|error| format!("Cannot inspect PDF copy permissions: {error}"))?;
    let text = if permitted {
        Arc::new(extract_text(&page, Some((&config, width, height)))?)
    } else {
        Arc::new(TextLayer {
            text: String::new(),
            glyphs: Vec::new(),
        })
    };
    Ok(RenderedPage {
        page: page_index,
        width,
        height,
        rgba,
        text,
    })
}

fn extract_text(
    page: &PdfPage<'_>,
    geometry: Option<(&PdfRenderConfig, u32, u32)>,
) -> Result<TextLayer, String> {
    let layer = page
        .text()
        .map_err(|error| format!("Cannot read PDF text: {error}"))?;
    let count = layer.len();
    if count < 0 {
        return Err("PDF text character count is invalid".into());
    }
    if count > MAX_GLYPHS {
        return Err("PDF page exceeds 200,000 text glyph limit".into());
    }
    let chars = layer.chars();
    let mut text = String::with_capacity(count as usize);
    let mut glyphs = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let character = chars
            .get(index)
            .map_err(|error| format!("Cannot read PDF character: {error}"))?;
        let start = text.len();
        let unicode = character
            .unicode_char()
            .filter(|ch| *ch != '\0')
            .unwrap_or('\u{FFFD}');
        text.push(unicode);
        let character_bounds = if geometry.is_some() && !unicode.is_whitespace() {
            character.loose_bounds().ok()
        } else {
            None
        };
        let bounds = match (geometry, character_bounds) {
            (Some((config, width, height)), Some(rect)) => {
                let points = [
                    (rect.left(), rect.top()),
                    (rect.right(), rect.top()),
                    (rect.left(), rect.bottom()),
                    (rect.right(), rect.bottom()),
                ];
                let mut xs = [0_f32; 4];
                let mut ys = [0_f32; 4];
                let mut valid = true;
                for (position, (x, y)) in points.into_iter().enumerate() {
                    if !x.value.is_finite() || !y.value.is_finite() {
                        valid = false;
                        break;
                    }
                    match page.points_to_pixels(x, y, config) {
                        Ok((px, py)) => {
                            xs[position] = px as f32 / width as f32;
                            ys[position] = py as f32 / height as f32;
                        }
                        Err(_) => {
                            valid = false;
                            break;
                        }
                    }
                }
                if valid {
                    let left = xs.into_iter().fold(f32::INFINITY, f32::min);
                    let right = xs.into_iter().fold(f32::NEG_INFINITY, f32::max);
                    let top = ys.into_iter().fold(f32::INFINITY, f32::min);
                    let bottom = ys.into_iter().fold(f32::NEG_INFINITY, f32::max);
                    let left = left.clamp(0.0, 1.0);
                    let right = right.clamp(0.0, 1.0);
                    let top = top.clamp(0.0, 1.0);
                    let bottom = bottom.clamp(0.0, 1.0);
                    (left < right && top < bottom).then_some(Rect {
                        left,
                        top,
                        right,
                        bottom,
                    })
                } else {
                    None
                }
            }
            _ => None,
        };
        glyphs.push(Glyph {
            start,
            end: text.len(),
            bounds,
        });
    }
    Ok(TextLayer { text, glyphs })
}

fn copy_text(
    document: &PdfDocument<'_>,
    selection: Option<Selection>,
    reply: &oneshot::Sender<Result<String, String>>,
) -> Result<String, String> {
    if !document
        .permissions()
        .can_extract_text_and_graphics()
        .map_err(|error| format!("Cannot inspect PDF copy permissions: {error}"))?
    {
        return Err("PDF permissions prohibit copying text".into());
    }
    let count = u32::try_from(document.pages().len())
        .map_err(|_| "PDF page count is invalid".to_string())?;
    let (start, end) = match selection {
        Some(selection) => {
            let (start, end) = if selection.anchor <= selection.focus {
                (selection.anchor, selection.focus)
            } else {
                (selection.focus, selection.anchor)
            };
            if start.page >= count || end.page >= count {
                return Err("PDF selection page is out of range".into());
            }
            (start.page, end.page)
        }
        None => (0, count - 1),
    };
    let mut result = String::new();
    for page_index in start..=end {
        if reply.is_canceled() {
            return Err("PDF copy was canceled".into());
        }
        let page = page_at(document, page_index)?;
        let layer = extract_text(&page, None)?;
        let (first, last) = match selection {
            Some(selection) => {
                let (from, to) = if selection.anchor <= selection.focus {
                    (selection.anchor, selection.focus)
                } else {
                    (selection.focus, selection.anchor)
                };
                let first = if page_index == from.page {
                    from.index
                } else {
                    0
                };
                let last = if page_index == to.page {
                    to.index
                        .checked_add(1)
                        .ok_or("PDF selection index overflow")?
                } else {
                    layer.glyphs.len()
                };
                if first > last
                    || last > layer.glyphs.len()
                    || (page_index == from.page && first == layer.glyphs.len())
                    || (page_index == to.page && last == 0)
                {
                    return Err("PDF selection character is out of range".into());
                }
                (first, last)
            }
            None => (0, layer.glyphs.len()),
        };
        let selected = if first == last {
            ""
        } else {
            &layer.text[layer.glyphs[first].start..layer.glyphs[last - 1].end]
        };
        if !selected.is_empty() {
            let separator = !result.is_empty();
            let added = selected.len() + usize::from(separator);
            if result
                .len()
                .checked_add(added)
                .is_none_or(|size| size > MAX_COPY_BYTES)
            {
                return Err("PDF copy exceeds 16 MiB limit".into());
            }
            if separator {
                result.push('\n');
            }
            result.push_str(selected);
        }
    }
    Ok(result)
}
