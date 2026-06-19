//! Image loading and processing functionality

use std::path::PathBuf;
use std::sync::mpsc::Sender;  // Multiple Producer, Single Consumer
use eframe::egui;
use egui::ColorImage;
use image::ImageReader;
use resvg;
use regex;
use turbojpeg::{Decompressor, Image as TurboImage, PixelFormat};

use crate::settings::ImageLoadingSettings;
use crate::file_locality::FileInfo;
use crate::benchmark::ImageCharacteristics;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageLoadStage {
    Loading,
    Decoding,
    Scaling,
    Uploading,
}

#[derive(Debug)]
pub enum ImageLoadUpdate {
    Stage { load_id: u64, stage: ImageLoadStage },
    Result { load_id: u64, result: Result<ImageLoadResult, String> },
}

#[derive(Debug)]
pub struct ImageLoadResult {
    pub color_image: ColorImage,
    pub texture_name: String,
    pub status_suffix: Option<String>,
}

pub fn should_skip_large_file(path: &PathBuf, settings: &ImageLoadingSettings, force_load: bool) -> Option<String> {
    // Check file locality status first to avoid any potential file access issues (unless forced)
    if !force_load {
        let file_info = FileInfo::new(path.clone());
        if file_info.will_trigger_download() {
            return Some(format!(
                "Skipped on-demand file: {}", 
                path.to_string_lossy()
            ));
        }
    }
    
    if let Some(max_mb) = settings.get_effective_max_file_size_mb() {
        if let Ok(metadata) = std::fs::metadata(path) {
            let size_mb = metadata.len() / (1024 * 1024);
            if size_mb > max_mb as u64 {
                let limit_source = if settings.max_file_size_mb.is_some() {
                    "manual"
                } else {
                    "dynamic"
                };
                return Some(format!(
                    "Skipped large file ({} MB > {} MB {} limit): {}",
                    size_mb, max_mb, limit_source, path.to_string_lossy()
                ));
            }
        }
    }
    None
}

pub fn scale_image_if_needed(img: image::DynamicImage, settings: &ImageLoadingSettings) -> Result<image::DynamicImage, String> {
    // Only scale if auto_scale_large_images is enabled and the image is considered "large"
    let (width, height) = (img.width(), img.height());
    
    const LARGE_IMAGE_THRESHOLD: u32 = 8192; // Arbitrary threshold for large images
    
    if width <= LARGE_IMAGE_THRESHOLD && height <= LARGE_IMAGE_THRESHOLD {
        return Ok(img);
    }

    if settings.skip_large_images {
        return Err(format!(
            "Image too large ({}x{} > {}x{} threshold)", 
            width, height, LARGE_IMAGE_THRESHOLD, LARGE_IMAGE_THRESHOLD
        ));
    }

    if settings.auto_scale_large_images {
        // Calculate scale factor to fit within threshold
        let scale_factor = (LARGE_IMAGE_THRESHOLD as f32 / width.max(height) as f32).min(1.0);
        let new_width = (width as f32 * scale_factor) as u32;
        let new_height = (height as f32 * scale_factor) as u32;

        // Use a faster scaling path for very large images.
        // Triangle interpolation is much faster than Lanczos3 while still giving reasonable results,
        // and thumbnail is often faster for large downscales than a full high-quality resize.
        if width > 12000 || height > 12000 || scale_factor < 0.6 {
            Ok(img.thumbnail(new_width, new_height))
        } else {
            Ok(img.resize(new_width, new_height, image::imageops::FilterType::Lanczos3))
        }
    } else {
        Err(format!(
            "Image too large ({}x{} > {}x{} threshold) and auto-scaling disabled", 
            width, height, LARGE_IMAGE_THRESHOLD, LARGE_IMAGE_THRESHOLD
        ))
    }
}

pub fn recolor_svg_simple(svg_content: &str, settings: &ImageLoadingSettings) -> String {
    if !settings.svg_recolor_enabled {
        return svg_content.to_string();
    }

    let target_hex = format!(
        "#{:02x}{:02x}{:02x}",
        settings.svg_target_color[0],
        settings.svg_target_color[1],
        settings.svg_target_color[2]
    );

    println!("SVG Recoloring enabled! Target color: {}", target_hex);
    println!("Original SVG preview: {}", &svg_content[..std::cmp::min(200, svg_content.len())]);

    let mut result = svg_content.to_string();
    let mut changes_made = 0;
    
    if result.contains("currentColor") {
        result = result.replace("currentColor", &target_hex);
        changes_made += result.matches(&target_hex).count();
        println!("Replaced currentColor with {}, {} instances", target_hex, changes_made);
    }
    
    // Match case insensitive fill colors, allowing for hex codes, named colors, and "none"
    let fill_regex = regex::Regex::new(r#"(?i)fill=(["'])(#[0-9a-f]{6}|#[0-9a-f]{3}|black|white|red|green|blue|yellow|cyan|magenta|purple|orange|brown|pink|gray|grey)\1"#).unwrap();
    let before_count = result.len();
    result = fill_regex.replace_all(&result, &format!(r#"fill="{}""#, target_hex)).to_string();
    if result.len() != before_count {
        changes_made += 1;
        println!("Replaced fill colors");
    }
        
    // Match case insensitive stroke colors, allowing for hex codes, named colors, and "none"
    let stroke_regex = regex::Regex::new(r#"(?i)stroke=(["'])(#[0-9a-f]{6}|#[0-9a-f]{3}|black|white|red|green|blue|yellow|cyan|magenta|purple|orange|brown|pink|gray|grey)\1"#).unwrap();
    let before_count = result.len();
    result = stroke_regex.replace_all(&result, &format!(r#"stroke="{}""#, target_hex)).to_string();
    if result.len() != before_count {
        changes_made += 1;
        println!("Replaced stroke colors");
    }

    // Match case insensitive style attributes that contain fill or stroke colors 
    let style_regex = regex::Regex::new(r#"(?i)style="[^"]*(?:fill|stroke):\s*(#[0-9a-f]{6}|#[0-9a-f]{3}|black|white|red|green|blue|yellow|cyan|magenta|currentColor)[^"]*""#).unwrap();
    let before_count = result.len();
    result = style_regex.replace_all(&result, &format!(r#"style="fill: {}; stroke: {};""#, target_hex, target_hex)).to_string();
    if result.len() != before_count {
        changes_made += 1;
        println!("Replaced CSS style colors");
    }

    println!("Total changes made: {}", changes_made);
    if changes_made > 0 {
        println!("Modified SVG preview: {}", &result[..std::cmp::min(200, result.len())]);
    }

    result
}

fn is_jpeg_path(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("jpg") || ext.eq_ignore_ascii_case("jpeg"))
        .unwrap_or(false)
}

fn load_jpeg_via_turbojpeg(path: &PathBuf) -> Result<image::DynamicImage, String> {
    let jpeg_data = std::fs::read(path)
        .map_err(|e| format!("Failed to read JPEG file: {}", e))?;

    let mut decompressor = Decompressor::new()
        .map_err(|e| format!("TurboJPEG init failed: {}", e))?;
    let header = decompressor.read_header(&jpeg_data)
        .map_err(|e| format!("TurboJPEG header read failed: {}", e))?;

    let width = header.width as u32;
    let height = header.height as u32;
    let pitch = header.width * PixelFormat::RGBA.size();
    let mut image_data = vec![0; pitch * header.height];

    let turbo_image = TurboImage {
        pixels: &mut image_data[..],
        width: header.width,
        pitch,
        height: header.height,
        format: PixelFormat::RGBA,
    };

    decompressor.decompress(&jpeg_data, turbo_image)
        .map_err(|e| format!("TurboJPEG decode failed: {}", e))?;

    let rgba_image = image::RgbaImage::from_raw(width, height, image_data)
        .ok_or_else(|| "TurboJPEG produced invalid RGBA buffer".to_string())?;
    Ok(image::DynamicImage::ImageRgba8(rgba_image))
}

/// Loads an SVG image - recolor and scale as needed, while respecting file locality to avoid triggering downloads.
pub fn load_svg_image(
    path: &PathBuf,
    settings: &ImageLoadingSettings,
    force_load: bool,
    stage_sender: Option<&Sender<ImageLoadUpdate>>,
    load_id: u64,
) -> Result<ImageLoadResult, String> {
    // Check file locality status first to avoid triggering downloads (unless forced)
    if !force_load {
        let file_info = FileInfo::new(path.clone());
        if file_info.will_trigger_download() {
            return Err("This is an on-demand file - would trigger download".to_string());
        }
    }
    
    if let Some(sender) = stage_sender {
        let _ = sender.send(ImageLoadUpdate::Stage { load_id, stage: ImageLoadStage::Loading });
    }

    let start = std::time::Instant::now();
    let svg_content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read SVG file: {}", e))?;

    // Apply recoloring if enabled
    let processed_svg = recolor_svg_simple(&svg_content, settings);
    
    if let Some(sender) = stage_sender {
        let _ = sender.send(ImageLoadUpdate::Stage {
            load_id,
            stage: ImageLoadStage::Decoding,
        });
    }

    let options = resvg::usvg::Options::default();
    let usvg_tree = resvg::usvg::Tree::from_str(&processed_svg, &options)
        .map_err(|e| format!("Failed to parse SVG: {}", e))?;
    
    let decode_time = start.elapsed();
    eprintln!("[ImageLoad] decode {:?} ms for {}", decode_time.as_millis(), path.display());

    let bbox = usvg_tree.size();
    let width = bbox.width() as u32;
    let height = bbox.height() as u32;
    
    // Handle very large SVGs
    const LARGE_SVG_THRESHOLD: u32 = 4096;
    let (scaled_width, scaled_height) = if width > LARGE_SVG_THRESHOLD || height > LARGE_SVG_THRESHOLD {
        if settings.auto_scale_large_images {
            if let Some(sender) = stage_sender {
                let _ = sender.send(ImageLoadUpdate::Stage { load_id, stage: ImageLoadStage::Scaling });
            }
            let scale_factor = (LARGE_SVG_THRESHOLD as f32 / width.max(height) as f32).min(1.0);
            ((width as f32 * scale_factor) as u32, (height as f32 * scale_factor) as u32)
        } else {
            return Err(format!("SVG too large ({}x{} > {}x{} threshold) and auto-scaling disabled", width, height, LARGE_SVG_THRESHOLD, LARGE_SVG_THRESHOLD));
        }
    } else {
        (width, height)
    };
    
    let mut pixmap = resvg::tiny_skia::Pixmap::new(scaled_width, scaled_height)
        .ok_or("Failed to create pixmap")?;
    
    let scale_start = std::time::Instant::now();
    let scale_x = scaled_width as f32 / width as f32;
    let scale_y = scaled_height as f32 / height as f32;
    let transform = resvg::tiny_skia::Transform::from_scale(scale_x, scale_y);
    
    resvg::render(&usvg_tree, transform, &mut pixmap.as_mut());
    let scale_time = scale_start.elapsed();
    eprintln!("[ImageLoad] svg render/scale {:?} ms for {}", scale_time.as_millis(), path.display());
    
    // Convert to RGBA
    let rgba_data: Vec<u8> = pixmap.data()
        .chunks_exact(4)
        .flat_map(|bgra| [bgra[2], bgra[1], bgra[0], bgra[3]]) // BGRA to RGBA
        .collect();
    
    let color_image = ColorImage::from_rgba_unmultiplied(
        [scaled_width as usize, scaled_height as usize],
        &rgba_data,
    );
    
    let texture_name = format!("svg_{}{}", path.file_name().unwrap_or_default().to_string_lossy(), if settings.svg_recolor_enabled { "_recolored" } else { "" });
    let status_suffix = if settings.svg_recolor_enabled {
        Some(" (recolored)".to_string())
    } else {
        None
    };

    Ok(ImageLoadResult {
        color_image,
        texture_name,
        status_suffix,
    })
}

pub fn load_raster_image(
    path: &PathBuf,
    settings: &ImageLoadingSettings,
    force_load: bool,
    stage_sender: Option<&Sender<ImageLoadUpdate>>,
    load_id: u64,
) -> Result<ImageLoadResult, String> {
    // Check file locality status first to avoid triggering downloads (unless forced)
    if !force_load {
        let file_info = FileInfo::new(path.clone());
        if file_info.will_trigger_download() {
            return Err("This is an on-demand file - would trigger download".to_string());
        }
    }
    
    if let Some(sender) = stage_sender {
        let _ = sender.send(ImageLoadUpdate::Stage { load_id, stage: ImageLoadStage::Loading });
    }
    let start = std::time::Instant::now();
    if let Some(sender) = stage_sender {
        let _ = sender.send(ImageLoadUpdate::Stage {
            load_id,
            stage: ImageLoadStage::Decoding,
        });
    }

    let img = if is_jpeg_path(path) {
        load_jpeg_via_turbojpeg(path)?
    } else {
        ImageReader::open(path)
            .map_err(|e| format!("Failed to open image: {}", e))?
            .decode()
            .map_err(|e| format!("Failed to decode image: {}", e))?
    };
    
    let decode_time = start.elapsed();
    eprintln!("[ImageLoad] decode {:?} ms for {}", decode_time.as_millis(), path.display());

    let (width, height) = (img.width(), img.height());
    if (width > 8192 || height > 8192) && settings.auto_scale_large_images {
        if let Some(sender) = stage_sender {
            let _ = sender.send(ImageLoadUpdate::Stage { load_id, stage: ImageLoadStage::Scaling });
        }
    }
    let scale_start = std::time::Instant::now();
    let scaled_img = scale_image_if_needed(img, settings)?;
    let scale_time = scale_start.elapsed();
    eprintln!("[ImageLoad] raster scale {:?} ms for {}", scale_time.as_millis(), path.display());
    
    let decode_and_scale_time = start.elapsed();
    eprintln!("[ImageLoad] raster decode+scale {:?} ms for {}", decode_and_scale_time.as_millis(), path.display());
    
    let size = [scaled_img.width() as _, scaled_img.height() as _];
    let rgba = scaled_img.to_rgba8();
    let pixels = rgba.as_flat_samples();
    let color_image = ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
    
    let texture_name = format!("image_{}", path.file_name().unwrap_or_default().to_string_lossy());

    Ok(ImageLoadResult {
        color_image,
        texture_name,
        status_suffix: None,
    })
}

pub fn estimate_image_render_time(path: &PathBuf, performance_profile: &crate::benchmark::PerformanceProfile) -> Option<f64> {
    // For on-demand files, skip dimension detection to avoid triggering downloads
    let file_info = FileInfo::new(path.clone());
    if file_info.will_trigger_download() {
        return None; // Cannot safely estimate without triggering download
    }
    
    // Try to get image dimensions without fully loading (safe for local files only)
    if let Ok(reader) = ImageReader::open(path) {
        if let Ok((width, height)) = reader.into_dimensions() {
            let format = path.extension()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_lowercase();
            
            let characteristics = ImageCharacteristics::new(path, width, height, format);
            let estimated_time = performance_profile.estimate_render_time(&characteristics);
            
            return Some(estimated_time);
        }
    }
    None
}
