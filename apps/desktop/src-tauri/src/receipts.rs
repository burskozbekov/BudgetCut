//! "Fiş ile Otomatik Fatura Kapama" — receipt-photo → Actuals.
//!
//! Native-only (needs the local filesystem + the user's own Anthropic key).
//! Pipeline: a single composite photo of several receipts is
//!   1. auto-segmented into per-receipt bounding boxes ([`segment`]),
//!   2. each box is cropped ([`crop_png`]) and sent to the Anthropic vision API
//!      (per crop, isolated) with a strict `tool_use` JSON schema so the model
//!      returns typed fields and never free-texts / hallucinates ([`extract`]),
//!   3. the user reviews + approves in the UI, then rows are written as Actuals.
//!
//! Every money/parse routine mirrors the rest of the app: TRY dates GG.AA.YYYY,
//! comma decimals; nothing is fabricated (unreadable → null + low confidence).

use std::collections::HashMap;
use std::io::Cursor;
use std::path::PathBuf;

use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use image::GenericImageView;
use serde::{Deserialize, Serialize};
use tauri::Manager;

// ---------------------------------------------------------------------------
// Settings (Anthropic API key + model), stored next to budget.db.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub anthropic_api_key: String,
    #[serde(default = "default_model")]
    pub anthropic_model: String,
}

fn default_model() -> String {
    "claude-sonnet-5".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            anthropic_api_key: String::new(),
            anthropic_model: default_model(),
        }
    }
}

/// What the UI is allowed to see — never hand the raw key back to the client.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsView {
    pub api_key_set: bool,
    pub model: String,
}

fn settings_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).ok();
    Ok(dir.join("settings.json"))
}

pub fn read_settings(app: &tauri::AppHandle) -> Settings {
    match settings_path(app).and_then(|p| std::fs::read_to_string(p).map_err(|e| e.to_string())) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

fn write_settings(app: &tauri::AppHandle, s: &Settings) -> Result<(), String> {
    let p = settings_path(app)?;
    let json = serde_json::to_string_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(p, json).map_err(|e| e.to_string())
}

pub fn settings_view(app: &tauri::AppHandle) -> SettingsView {
    let s = read_settings(app);
    SettingsView {
        api_key_set: !s.anthropic_api_key.trim().is_empty(),
        model: s.anthropic_model,
    }
}

/// Update settings. `None` leaves a field unchanged; an empty api_key clears it.
pub fn update_settings(
    app: &tauri::AppHandle,
    api_key: Option<String>,
    model: Option<String>,
) -> Result<SettingsView, String> {
    let mut s = read_settings(app);
    if let Some(k) = api_key {
        s.anthropic_api_key = k.trim().to_string();
    }
    if let Some(m) = model {
        let m = m.trim();
        if !m.is_empty() {
            s.anthropic_model = m.to_string();
        }
    }
    write_settings(app, &s)?;
    Ok(SettingsView {
        api_key_set: !s.anthropic_api_key.trim().is_empty(),
        model: s.anthropic_model,
    })
}

// ---------------------------------------------------------------------------
// Image decode + segmentation.
// ---------------------------------------------------------------------------

/// A receipt region in composite-pixel coordinates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct BBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentResult {
    pub width: u32,
    pub height: u32,
    pub boxes: Vec<BBox>,
}

/// Decode an image with hard resource limits so a decompression-bomb (small on
/// disk, enormous decoded) yields a graceful error instead of OOM-aborting the
/// process. Used at every decode site (segment + crop).
fn load_limited(bytes: &[u8]) -> Result<image::DynamicImage, String> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("görsel biçimi okunamadı: {e}"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_000);
    limits.max_image_height = Some(16_000);
    limits.max_alloc = Some(768 * 1024 * 1024); // 768 MB decoded ceiling
    reader.limits(limits);
    reader
        .decode()
        .map_err(|e| format!("görsel çözülemedi (çok büyük veya bozuk olabilir): {e}"))
}

/// Strip an optional `data:...;base64,` prefix and decode.
pub fn decode_image_b64(b64: &str) -> Result<Vec<u8>, String> {
    let data = match b64.find(",") {
        Some(i) if b64.starts_with("data:") => &b64[i + 1..],
        _ => b64,
    };
    STANDARD
        .decode(data.trim())
        .map_err(|e| format!("görsel çözümlenemedi: {e}"))
}

/// Heuristic auto-segmentation: receipts read as large light blobs against a
/// darker table. Otsu-threshold the luma, label connected components, and keep
/// the big, roughly-rectangular ones. This is deliberately conservative — the
/// UI always lets the user add/drag/remove boxes when the guess is off.
pub fn segment(img_bytes: &[u8]) -> Result<SegmentResult, String> {
    let img = load_limited(img_bytes)?;
    let (w, h) = img.dimensions();
    let gray = img.to_luma8();
    let level = imageproc::contrast::otsu_level(&gray);
    // Receipts are the brighter regions → foreground = pixels above Otsu.
    let bin =
        imageproc::contrast::threshold(&gray, level, imageproc::contrast::ThresholdType::Binary);
    let labels = imageproc::region_labelling::connected_components(
        &bin,
        imageproc::region_labelling::Connectivity::Eight,
        image::Luma([0u8]),
    );

    // Per-label bounding box.
    let mut min_x: HashMap<u32, u32> = HashMap::new();
    let mut min_y: HashMap<u32, u32> = HashMap::new();
    let mut max_x: HashMap<u32, u32> = HashMap::new();
    let mut max_y: HashMap<u32, u32> = HashMap::new();
    let mut area: HashMap<u32, u32> = HashMap::new();
    for y in 0..h {
        for x in 0..w {
            let l = labels.get_pixel(x, y).0[0];
            if l == 0 {
                continue; // background
            }
            min_x.entry(l).and_modify(|v| *v = (*v).min(x)).or_insert(x);
            min_y.entry(l).and_modify(|v| *v = (*v).min(y)).or_insert(y);
            max_x.entry(l).and_modify(|v| *v = (*v).max(x)).or_insert(x);
            max_y.entry(l).and_modify(|v| *v = (*v).max(y)).or_insert(y);
            *area.entry(l).or_insert(0) += 1;
        }
    }

    let total = (w as u64) * (h as u64);
    let min_area = (total / 40).max(1); // a receipt is ≥ ~2.5% of the frame
    let mut boxes: Vec<BBox> = Vec::new();
    for (l, a) in &area {
        if (*a as u64) < min_area {
            continue;
        }
        let (x0, y0, x1, y1) = (min_x[l], min_y[l], max_x[l], max_y[l]);
        let bw = x1 - x0 + 1;
        let bh = y1 - y0 + 1;
        // Skip the whole-frame blob (a light background labeled as one region).
        if (bw as u64) * (bh as u64) > total * 9 / 10 {
            continue;
        }
        // A little padding, clamped to the frame.
        let pad_x = bw / 20;
        let pad_y = bh / 20;
        let nx = x0.saturating_sub(pad_x);
        let ny = y0.saturating_sub(pad_y);
        boxes.push(BBox {
            x: nx,
            y: ny,
            w: (bw + 2 * pad_x).min(w - nx),
            h: (bh + 2 * pad_y).min(h - ny),
        });
    }
    // Reading order: top-to-bottom, then left-to-right (banded).
    boxes.sort_by_key(|b| (b.y / (h / 8).max(1), b.x));
    Ok(SegmentResult {
        width: w,
        height: h,
        boxes,
    })
}

/// Crop `bbox` out of the composite and re-encode as PNG (clamped to bounds).
pub fn crop_png(img_bytes: &[u8], bbox: BBox) -> Result<Vec<u8>, String> {
    let img = load_limited(img_bytes)?;
    let (w, h) = img.dimensions();
    let x = bbox.x.min(w.saturating_sub(1));
    let y = bbox.y.min(h.saturating_sub(1));
    let cw = bbox.w.min(w - x).max(1);
    let ch = bbox.h.min(h - y).max(1);
    let cropped = img.crop_imm(x, y, cw, ch);
    let mut out = Vec::new();
    cropped
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("kırpma kodlanamadı: {e}"))?;
    Ok(out)
}

/// Wrap PNG bytes as a `data:image/png;base64,…` URL for an `<img>` thumbnail.
#[must_use]
pub fn png_data_url(bytes: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(bytes))
}

// ---------------------------------------------------------------------------
// Anthropic vision extraction (per crop, strict tool_use schema).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ReceiptFields {
    #[serde(default)]
    pub tedarikci: Option<String>,
    #[serde(default)]
    pub tarih: Option<String>,
    #[serde(default)]
    pub tutar_net: Option<f64>,
    #[serde(default)]
    pub kdv_tutari: Option<f64>,
    #[serde(default)]
    pub toplam_tutar: Option<f64>,
    #[serde(default = "default_para")]
    pub para_birimi: String,
    #[serde(default)]
    pub aciklama_onerisi: Option<String>,
    #[serde(default)]
    pub hesap_kodu_onerisi: Option<String>,
    #[serde(default)]
    pub alan_guven_skorlari: HashMap<String, f64>,
    #[serde(default)]
    pub ham_ocr_metni: String,
}

fn default_para() -> String {
    "TRY".to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractResult {
    pub fields: ReceiptFields,
    /// The cropped receipt as a `data:image/png;base64,…` URL for the thumbnail.
    pub crop_data_url: String,
}

/// An account the model may fuzzy-match `hesap_kodu_onerisi` against.
#[derive(Debug, Clone, Deserialize)]
pub struct AccountHint {
    pub number: String,
    pub name: String,
}

/// The `tool_use` input schema forcing strict, typed, no-hallucination output.
fn tool_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "tedarikci": {"type": ["string", "null"]},
            "tarih": {"type": ["string", "null"], "description": "GG.AA.YYYY"},
            "tutar_net": {"type": ["number", "null"]},
            "kdv_tutari": {"type": ["number", "null"]},
            "toplam_tutar": {"type": ["number", "null"]},
            "para_birimi": {"type": "string", "enum": ["TRY", "USD", "EUR"]},
            "aciklama_onerisi": {"type": ["string", "null"]},
            "hesap_kodu_onerisi": {"type": ["string", "null"]},
            "alan_guven_skorlari": {"type": "object", "additionalProperties": {"type": "number"}},
            "ham_ocr_metni": {"type": "string"}
        },
        "required": ["para_birimi", "alan_guven_skorlari", "ham_ocr_metni"]
    })
}

fn build_prompt(accounts: &[AccountHint]) -> String {
    let mut list = String::new();
    for a in accounts.iter().take(300) {
        list.push_str(&format!("{} {}\n", a.number, a.name));
    }
    format!(
        "Bu görselde TEK bir fiş/fatura var. Alanlarını `fis_kaydet` aracıyla çıkar. \
KURALLAR: Hiçbir alanı UYDURMA. Okuyamadığın/emin olmadığın alanı null bırak ve o \
alanın güven skorunu (alan_guven_skorlari) düşük ver (0.0–1.0). Tarihi GG.AA.YYYY \
biçiminde ver. Tutarları sayı olarak ver (Türkçe virgüllü tutarları noktaya çevir). \
ham_ocr_metni alanına fişte okuduğun ham metni yaz. hesap_kodu_onerisi için aşağıdaki \
hesap planından en uygun KODU seç; emin değilsen null bırak.\n\nHESAP PLANI:\n{list}"
    )
}

/// Locate the tool_use block in an Anthropic /v1/messages response and parse
/// its `input` into [`ReceiptFields`]. Pure — unit-tested with a canned body.
pub fn parse_tool_response(body: &serde_json::Value) -> Result<ReceiptFields, String> {
    let content = body
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or("beklenmeyen API yanıtı (content yok)")?;
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
            let input = block.get("input").ok_or("tool_use input yok")?;
            return serde_json::from_value(input.clone())
                .map_err(|e| format!("alanlar çözümlenemedi: {e}"));
        }
    }
    // No tool call → surface the model's text or a stop reason rather than a
    // silent empty row.
    let reason = body
        .get("stop_reason")
        .and_then(|r| r.as_str())
        .unwrap_or("bilinmiyor");
    Err(format!(
        "model fiş alanlarını döndürmedi (stop_reason: {reason})"
    ))
}

/// Blocking call to the Anthropic messages API for one receipt crop. Returns a
/// clear error on network/auth/limit failures (never a silent empty result).
pub fn extract_blocking(
    api_key: &str,
    model: &str,
    crop_png_bytes: &[u8],
    accounts: &[AccountHint],
) -> Result<ReceiptFields, String> {
    if api_key.trim().is_empty() {
        return Err("Anthropic API anahtarı ayarlanmamış (Ayarlar).".into());
    }
    let b64 = STANDARD.encode(crop_png_bytes);
    let req = serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "tools": [{
            "name": "fis_kaydet",
            "description": "Bir fiş/faturadan çıkarılan alanları kaydet.",
            "input_schema": tool_schema(),
        }],
        "tool_choice": {"type": "tool", "name": "fis_kaydet"},
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64}},
                {"type": "text", "text": build_prompt(accounts)}
            ]
        }]
    });

    let resp = ureq::post("https://api.anthropic.com/v1/messages")
        .set("x-api-key", api_key.trim())
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .timeout(std::time::Duration::from_secs(60))
        .send_json(req);

    match resp {
        Ok(r) => {
            let body: serde_json::Value = r
                .into_json()
                .map_err(|e| format!("API yanıtı okunamadı: {e}"))?;
            parse_tool_response(&body)
        }
        Err(ureq::Error::Status(code, r)) => {
            let msg = r.into_string().unwrap_or_default();
            let hint = match code {
                401 => "API anahtarı geçersiz",
                429 => "hız/limit aşıldı, biraz sonra tekrar dene",
                529 => "servis geçici olarak meşgul",
                _ => "API hatası",
            };
            Err(format!(
                "{hint} (HTTP {code}): {}",
                msg.chars().take(300).collect::<String>()
            ))
        }
        Err(e) => Err(format!("ağ hatası: {e}")),
    }
}

// ---------------------------------------------------------------------------
// Evidence attachments — composite + crop saved keyed by the Actual's id.
// ---------------------------------------------------------------------------

fn attachments_dir(app: &tauri::AppHandle, actual_id: &str) -> Result<PathBuf, String> {
    // Guard against path traversal: the id must be a plain uuid.
    if actual_id.is_empty() || actual_id.contains(['/', '\\', '.']) {
        return Err("geçersiz kayıt kimliği".into());
    }
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("attachments").join(actual_id))
}

pub fn save_attachment(
    app: &tauri::AppHandle,
    actual_id: &str,
    composite_b64: &str,
    crop_b64: &str,
) -> Result<(), String> {
    let dir = attachments_dir(app, actual_id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let composite = decode_image_b64(composite_b64)?;
    let crop = decode_image_b64(crop_b64)?;
    std::fs::write(dir.join("composite.jpg"), composite).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("crop.png"), crop).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn remove_attachment(app: &tauri::AppHandle, actual_id: &str) -> Result<(), String> {
    let dir = attachments_dir(app, actual_id)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_response_extracts_fields() {
        let body = serde_json::json!({
            "content": [
                {"type": "text", "text": "işte fiş"},
                {"type": "tool_use", "name": "fis_kaydet", "input": {
                    "tedarikci": "Migros",
                    "tarih": "05.03.2021",
                    "tutar_net": 1000.0,
                    "kdv_tutari": 100.0,
                    "toplam_tutar": 1100.0,
                    "para_birimi": "TRY",
                    "aciklama_onerisi": "market",
                    "hesap_kodu_onerisi": "2100",
                    "alan_guven_skorlari": {"tutar_net": 0.4},
                    "ham_ocr_metni": "MIGROS ... TOPLAM 1.100,00"
                }}
            ],
            "stop_reason": "tool_use"
        });
        let f = parse_tool_response(&body).unwrap();
        assert_eq!(f.tedarikci.as_deref(), Some("Migros"));
        assert_eq!(f.tarih.as_deref(), Some("05.03.2021"));
        assert_eq!(f.toplam_tutar, Some(1100.0));
        assert_eq!(f.para_birimi, "TRY");
        assert_eq!(f.alan_guven_skorlari.get("tutar_net").copied(), Some(0.4));
    }

    #[test]
    fn parse_tool_response_errors_when_no_tool_call() {
        let body = serde_json::json!({
            "content": [{"type": "text", "text": "okuyamadım"}],
            "stop_reason": "end_turn"
        });
        assert!(parse_tool_response(&body).is_err());
    }

    #[test]
    fn decode_strips_data_url_prefix() {
        // "hi" base64 = aGk=
        assert_eq!(
            decode_image_b64("data:image/png;base64,aGk=").unwrap(),
            b"hi"
        );
        assert_eq!(decode_image_b64("aGk=").unwrap(), b"hi");
    }

    #[test]
    fn segment_finds_two_receipts_on_dark_table() {
        // Dark 200×100 canvas with two bright 60×60 rectangles.
        let mut img = image::RgbImage::from_pixel(200, 100, image::Rgb([20, 20, 20]));
        for (rx, _) in [(20u32, 0u32), (120u32, 0u32)] {
            for y in 20..80 {
                for x in rx..rx + 60 {
                    img.put_pixel(x, y, image::Rgb([240, 240, 240]));
                }
            }
        }
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        let seg = segment(&bytes).unwrap();
        assert_eq!(seg.width, 200);
        assert_eq!(seg.boxes.len(), 2, "should find the two bright receipts");
        // Left receipt comes first in reading order.
        assert!(seg.boxes[0].x < seg.boxes[1].x);
    }

    #[test]
    fn crop_png_clamps_to_bounds() {
        let img = image::RgbImage::from_pixel(100, 100, image::Rgb([0, 0, 0]));
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        // Box partly outside the frame — must not panic, must clamp.
        let out = crop_png(
            &bytes,
            BBox {
                x: 80,
                y: 80,
                w: 50,
                h: 50,
            },
        )
        .unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        assert_eq!(decoded.dimensions(), (20, 20));
    }
}
