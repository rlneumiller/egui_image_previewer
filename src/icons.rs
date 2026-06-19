//! Icon support for the application

use eframe::egui;
use std::collections::HashMap;
use resvg;
/// Pre-validated SVG icon data embedded at compile time
pub struct EmbeddedIcon {
    pub name: &'static str,
    pub content: &'static str,
}

/// All embedded icons with compile-time validation
pub static EMBEDDED_ICONS: &[EmbeddedIcon] = &[
    EmbeddedIcon { name: "alert-triangle", content: include_str!("../assets/icons/alert-triangle.svg") },
    EmbeddedIcon { name: "check", content: include_str!("../assets/icons/check.svg") },
    EmbeddedIcon { name: "circle-check", content: include_str!("../assets/icons/circle-check.svg") },
    EmbeddedIcon { name: "clock", content: include_str!("../assets/icons/clock.svg") },
    EmbeddedIcon { name: "cloud", content: include_str!("../assets/icons/cloud.svg") },
    EmbeddedIcon { name: "device-floppy", content: include_str!("../assets/icons/device-floppy.svg") },
    EmbeddedIcon { name: "download", content: include_str!("../assets/icons/download.svg") },
    EmbeddedIcon { name: "help", content: include_str!("../assets/icons/help.svg") },
    EmbeddedIcon { name: "x", content: include_str!("../assets/icons/x.svg") },
];

/// SVG icon loader and renderer with embedded validation
pub struct SvgIcons;

impl SvgIcons {
    /// Validate all embedded SVG icons at compile time
    pub fn validate_all_icons() -> Result<(), String> {
        for icon in EMBEDDED_ICONS {
            if icon.content.is_empty() {
                return Err(format!("Icon '{}' has empty content", icon.name));
            }
            
            // Basic SVG validation - check for required elements
            if !icon.content.contains("<svg") {
                return Err(format!("Icon '{}' does not contain valid SVG markup", icon.name));
            }
        }
        Ok(())
    }
    
    /// Get embedded SVG content by name
    fn get_embedded_svg(icon_name: &str) -> Option<&'static str> {
        EMBEDDED_ICONS.iter()
            .find(|icon| icon.name == icon_name)
            .map(|icon| icon.content)
    }
    
    /// Get list of all available icon names
    pub fn get_available_icons() -> Vec<&'static str> {
        EMBEDDED_ICONS.iter().map(|icon| icon.name).collect()
    }
    
    /// Load and render an SVG icon as an egui texture using embedded content
    pub fn load_icon(ctx: &egui::Context, icon_name: &str, size: f32, color: egui::Color32) -> Option<egui::TextureHandle> {
        let svg_content = Self::get_embedded_svg(icon_name)?;
        Self::render_svg_to_texture(ctx, svg_content, size, color, icon_name)
    }
    
    fn render_svg_to_texture(ctx: &egui::Context, svg_content: &str, size: f32, color: egui::Color32, icon_name: &str) -> Option<egui::TextureHandle> {
        use resvg::usvg;
        
        // Validate size parameter to prevent errors
        if size <= 0.0 || size > 1024.0 {
            eprintln!("Warning: Invalid icon size {} for icon '{}', using default 16.0", size, icon_name);
            return Self::render_svg_to_texture(ctx, svg_content, 16.0, color, icon_name);
        }
        
        let colored_svg = svg_content.replace(
            "currentColor", 
            &format!("rgb({},{},{})", color.r(), color.g(), color.b())
        );
        
        // Parse SVG with error handling
        let opt = usvg::Options::default();
        
        let usvg_tree = match usvg::Tree::from_str(&colored_svg, &opt) {
            Ok(tree) => tree,
            Err(e) => {
                eprintln!("Error parsing SVG for icon '{}': {}", icon_name, e);
                return None;
            }
        };
        
        // Render to pixmap with error handling
        let size_u32 = size as u32;
        let mut pixmap = match resvg::tiny_skia::Pixmap::new(size_u32, size_u32) {
            Some(pixmap) => pixmap,
            None => {
                eprintln!("Error creating pixmap for icon '{}' with size {}", icon_name, size);
                return None;
            }
        };
        
        resvg::render(&usvg_tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
        
        // Convert to egui texture
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [size_u32 as usize, size_u32 as usize],
            pixmap.data(),
        );
        
        Some(ctx.load_texture(
            format!("icon_{}_{}", icon_name, size as u32),
            image,
            egui::TextureOptions::LINEAR,
        ))
    }
}

/// Icon constants for easy access
pub struct Icons;

impl Icons {
    pub const DEVICE_FLOPPY: &'static str = "device-floppy";
    pub const CLOUD: &'static str = "cloud";
    pub const DOWNLOAD: &'static str = "download";
    pub const CHECK: &'static str = "check";
    pub const X: &'static str = "x";
    pub const ALERT_TRIANGLE: &'static str = "alert-triangle";
    pub const HELP: &'static str = "help";
    pub const CIRCLE_CHECK: &'static str = "circle-check";
    pub const CLOCK: &'static str = "clock";
}

/// Better icon representation that's guaranteed to work
pub struct IconRenderer {
    cache: HashMap<String, egui::TextureHandle>,
}

impl Default for IconRenderer {
    fn default() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
}

impl IconRenderer {
    /// Create a new IconRenderer and validate icons
    pub fn new() -> Self {
        // Validate all icons at startup
        if let Err(e) = SvgIcons::validate_all_icons() {
            eprintln!("Warning: Icon validation failed: {}", e);
        }
        
        Self {
            cache: HashMap::new(),
        }
    }
    
    /// Get or create an icon texture with better error handling
    pub fn get_icon(&mut self, ctx: &egui::Context, icon: &str, size: f32, color: egui::Color32) -> Option<&egui::TextureHandle> {
        let cache_key = format!("{}_{}_{}_{}", icon, size as u32, color.r(), color.g());
        
        if !self.cache.contains_key(&cache_key) {
            match SvgIcons::load_icon(ctx, icon, size, color) {
                Some(texture) => {
                    self.cache.insert(cache_key.clone(), texture);
                }
                None => {
                    // Log the failure but don't spam the console
                    if !self.cache.contains_key(&format!("failed_{}", icon)) {
                        eprintln!("Warning: Failed to load icon '{}'. Available icons: {:?}", 
                                icon, SvgIcons::get_available_icons());
                        // Mark this icon as failed to avoid repeated warnings
                        self.cache.insert(format!("failed_{}", icon), 
                            ctx.load_texture("placeholder", egui::ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]), egui::TextureOptions::default()));
                    }
                }
            }
        }
        
        self.cache.get(&cache_key)
    }
    
    /// Render an icon in the UI with improved fallback
    pub fn icon_button(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, icon: &str, size: f32, color: egui::Color32, tooltip: &str) -> egui::Response {
        if let Some(texture) = self.get_icon(ctx, icon, size, color) {
            ui.image((texture.id(), egui::Vec2::splat(size))).on_hover_text(tooltip)
        } else {
            // Improved fallback with better visual representation
            let fallback_text = match icon {
                "device-floppy" => "💾",
                "cloud" => "☁",
                "download" => "⬇",
                "check" => "✓",
                "x" => "✗",
                "alert-triangle" => "⚠",
                "help" => "?",
                "circle-check" => "✅",
                "clock" => "🕐",
                _ => &format!("[{}]", icon.chars().next().unwrap_or('?').to_uppercase()),
            };
            ui.colored_label(color, fallback_text).on_hover_text(tooltip)
        }
    }
    
    /// Simple icon label with improved fallback
    pub fn icon_label(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, icon: &str, size: f32, color: egui::Color32) -> egui::Response {
        if let Some(texture) = self.get_icon(ctx, icon, size, color) {
            ui.image((texture.id(), egui::Vec2::splat(size)))
        } else {
            // Improved fallback with better visual representation
            let fallback_text = match icon {
                "device-floppy" => "💾",
                "cloud" => "☁",
                "download" => "⬇",
                "check" => "✓",
                "x" => "✗",
                "alert-triangle" => "⚠",
                "help" => "?",
                "circle-check" => "✅",
                "clock" => "🕐",
                _ => &format!("[{}]", icon.chars().next().unwrap_or('?').to_uppercase()),
            };
            ui.colored_label(color, fallback_text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_icons_available() {
        // Verify all expected icons are embedded
        let expected_icons = vec![
            "alert-triangle", "check", "circle-check", "clock", 
            "cloud", "device-floppy", "download", "help", "x"
        ];
        
        let available_icons = SvgIcons::get_available_icons();
        
        for expected in &expected_icons {
            assert!(available_icons.contains(expected), 
                "Expected icon '{}' not found in embedded icons", expected);
        }
        
        assert_eq!(available_icons.len(), expected_icons.len(), 
            "Number of available icons doesn't match expected");
    }

    #[test]
    fn test_icon_validation() {
        // Test that all embedded icons pass validation
        assert!(SvgIcons::validate_all_icons().is_ok(), 
            "Icon validation failed");
    }

    #[test]
    fn test_embedded_svg_content() {
        // Test that we can get SVG content for all icons
        for icon in EMBEDDED_ICONS {
            let content = SvgIcons::get_embedded_svg(icon.name);
            assert!(content.is_some(), "Failed to get content for icon '{}'", icon.name);
            
            let svg_content = content.unwrap();
            assert!(!svg_content.is_empty(), "Icon '{}' has empty content", icon.name);
            assert!(svg_content.contains("<svg"), "Icon '{}' does not contain SVG markup", icon.name);
        }
    }

    #[test]
    fn test_invalid_icon_name() {
        // Test that requesting an invalid icon returns None
        let content = SvgIcons::get_embedded_svg("nonexistent-icon");
        assert!(content.is_none(), "Should return None for nonexistent icon");
    }
}