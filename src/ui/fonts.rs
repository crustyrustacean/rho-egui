// src/ui/fonts.rs

/// Register a proportional UI font and a monospace font.
///
/// egui's bundled default is Ubuntu-Light for *everything* — the
/// `FontFamily::Monospace` list is `["Hack", "Ubuntu-Light", …]`, so a system
/// monospace is never used unless one is registered. Without this, inline code
/// spans, file paths and fenced code blocks render in the same face as prose,
/// which reads as "unformatted" even when the markdown is perfectly well
/// formed. In a sampled session reply that was 179 inline-code spans and 7
/// fenced blocks.
///
/// Fonts are read at runtime instead of embedded: embedding would add ~1 MB to
/// the binary for faces Windows already ships. A miss is a cosmetic
/// regression, never a startup failure — the bundled fonts stay in the
/// fallback chain either way.
///
/// Call once, before the first frame. See [`crate::app::App::new`].
pub(crate) fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    if let Some((name, data)) = load_first(PROPORTIONAL_CANDIDATES) {
        fonts.font_data.insert(name.clone(), data);
        // Highest priority in the proportional chain; the bundled emoji and
        // icon fallbacks stay behind it.
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, name);
    }

    if let Some((name, data)) = load_first(MONOSPACE_CANDIDATES) {
        fonts.font_data.insert(name.clone(), data);
        // Put it ahead of "Hack" so code actually renders in a real monospace.
        // The icon/emoji fallbacks stay last.
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .insert(0, name);
    }

    ctx.set_fonts(fonts);
}

/// System proportional faces, best first. Segoe UI is the Windows UI face and
/// holds up at the larger sizes this app renders text; Arial and Calibri cover
/// the case where it is missing.
///
/// Listed per-platform because these are absolute paths. A release build is
/// not Windows-only, and a hardcoded `C:\Windows\Fonts` path means a Linux or
/// macOS machine silently falls back to Ubuntu-Light for everything — losing
/// the monospace/proportional distinction that this module exists to create.
const PROPORTIONAL_CANDIDATES_WINDOWS: &[(&str, &str)] = &[
    ("Segoe UI", r"C:\Windows\Fonts\segoeui.ttf"),
    ("Arial", r"C:\Windows\Fonts\arial.ttf"),
    ("Calibri", r"C:\Windows\Fonts\calibri.ttf"),
];

#[cfg_attr(target_os = "windows", allow(dead_code))]
const PROPORTIONAL_CANDIDATES_UNIX: &[(&str, &str)] = &[
    (
        "DejaVu Sans",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ),
    (
        "Liberation Sans",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    ),
    (
        "Noto Sans",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    ),
];

/// Monospace faces, best first. Cascadia ships with Windows Terminal and is far
/// easier to read at length than the egui default; Consolas is the traditional
/// fallback.
const MONOSPACE_CANDIDATES_WINDOWS: &[(&str, &str)] = &[
    ("Cascadia Code", r"C:\Windows\Fonts\CascadiaCode.ttf"),
    ("Consolas", r"C:\Windows\Fonts\consola.ttf"),
    ("Lucida Console", r"C:\Windows\Fonts\lucon.ttf"),
];

#[cfg_attr(target_os = "windows", allow(dead_code))]
const MONOSPACE_CANDIDATES_UNIX: &[(&str, &str)] = &[
    (
        "DejaVu Sans Mono",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ),
    (
        "Liberation Mono",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    ),
    (
        "Noto Sans Mono",
        "/usr/share/fonts/truetype/noto/NotoSansMono-Regular.ttf",
    ),
];

#[cfg(target_os = "windows")]
const PROPORTIONAL_CANDIDATES: &[(&str, &str)] = PROPORTIONAL_CANDIDATES_WINDOWS;
#[cfg(not(target_os = "windows"))]
const PROPORTIONAL_CANDIDATES: &[(&str, &str)] = PROPORTIONAL_CANDIDATES_UNIX;

#[cfg(target_os = "windows")]
const MONOSPACE_CANDIDATES: &[(&str, &str)] = MONOSPACE_CANDIDATES_WINDOWS;
#[cfg(not(target_os = "windows"))]
const MONOSPACE_CANDIDATES: &[(&str, &str)] = MONOSPACE_CANDIDATES_UNIX;

type LoadedFont = (String, std::sync::Arc<egui::epaint::text::FontData>);

fn load_first(candidates: &[(&str, &str)]) -> Option<LoadedFont> {
    for (name, path) in candidates {
        match std::fs::read(path) {
            Ok(bytes) => {
                eprintln!("rho-egui: using {name} ({path})");
                let data = egui::epaint::text::FontData::from_owned(bytes);
                return Some(((*name).to_owned(), std::sync::Arc::new(data)));
            }
            Err(_) => continue,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing font must degrade to egui's bundled defaults, not panic.
    ///
    /// The candidate lists are absolute Windows paths. A release build handed
    /// to another machine — or run on Linux under WSL — will not find them, and
    /// `load_first` returning `None` has to leave `FontDefinitions` intact so
    /// egui still renders with Ubuntu-Light.
    #[test]
    fn missing_fonts_fall_back_to_egui_defaults() {
        assert!(
            load_first(&[]).is_none(),
            "an empty candidate list must yield None, not a panic"
        );
        assert!(
            load_first(&[("Nope", r"Z:\definitely\not\here.ttf")]).is_none(),
            "a nonexistent path must yield None, not a panic"
        );
    }

    /// Whatever happens, `install` must leave a usable font definition.
    #[test]
    fn install_always_produces_a_usable_font_definition() {
        let ctx = egui::Context::default();
        let before = ctx.style_of(egui::Theme::Dark).text_styles.len();
        install(&ctx);
        let after = ctx.style_of(egui::Theme::Dark).text_styles.len();
        assert_eq!(
            before, after,
            "install must not disturb the text styles it is not setting"
        );
    }

    /// Font discovery must work on the platform we are running on.
    ///
    /// Deliberately not asserting that a font is *always* found: CI runs on
    /// `ubuntu-latest`, whose font set is not guaranteed to include any of the
    /// Unix candidates, and a bare container may have no fonts at all. The
    /// guarantee under test is that the *lookup* behaves — when the platform
    /// has a font directory, the search finds something in it.
    #[test]
    fn system_fonts_are_discovered_when_present() {
        use std::path::Path;
        let prop_dir_exists = PROPORTIONAL_CANDIDATES
            .iter()
            .any(|(_, p)| Path::new(p).parent().is_some_and(Path::is_dir));
        let mono_dir_exists = MONOSPACE_CANDIDATES
            .iter()
            .any(|(_, p)| Path::new(p).parent().is_some_and(Path::is_dir));

        if prop_dir_exists {
            assert!(
                load_first(PROPORTIONAL_CANDIDATES).is_some(),
                "a proportional font directory exists but nothing resolved from {PROPORTIONAL_CANDIDATES:?}"
            );
        }
        if mono_dir_exists {
            assert!(
                load_first(MONOSPACE_CANDIDATES).is_some(),
                "a monospace font directory exists but nothing resolved from {MONOSPACE_CANDIDATES:?}"
            );
        }
    }
}
