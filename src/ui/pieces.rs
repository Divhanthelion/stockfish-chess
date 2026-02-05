use egui::{vec2, Color32, ColorImage, Context, TextureHandle, TextureOptions};
use shakmaty::{Color, Role};
use std::collections::HashMap;

// Embedded SVG piece data
const PIECE_SVGS: &[(&str, &str)] = &[
    ("wp", include_str!("../assets/pieces/wp.svg")),
    ("wn", include_str!("../assets/pieces/wn.svg")),
    ("wb", include_str!("../assets/pieces/wb.svg")),
    ("wr", include_str!("../assets/pieces/wr.svg")),
    ("wq", include_str!("../assets/pieces/wq.svg")),
    ("wk", include_str!("../assets/pieces/wk.svg")),
    ("bp", include_str!("../assets/pieces/bp.svg")),
    ("bn", include_str!("../assets/pieces/bn.svg")),
    ("bb", include_str!("../assets/pieces/bb.svg")),
    ("br", include_str!("../assets/pieces/br.svg")),
    ("bq", include_str!("../assets/pieces/bq.svg")),
    ("bk", include_str!("../assets/pieces/bk.svg")),
];

fn piece_key(role: Role, color: Color) -> &'static str {
    match (color, role) {
        (Color::White, Role::Pawn) => "wp",
        (Color::White, Role::Knight) => "wn",
        (Color::White, Role::Bishop) => "wb",
        (Color::White, Role::Rook) => "wr",
        (Color::White, Role::Queen) => "wq",
        (Color::White, Role::King) => "wk",
        (Color::Black, Role::Pawn) => "bp",
        (Color::Black, Role::Knight) => "bn",
        (Color::Black, Role::Bishop) => "bb",
        (Color::Black, Role::Rook) => "br",
        (Color::Black, Role::Queen) => "bq",
        (Color::Black, Role::King) => "bk",
    }
}

pub struct PieceRenderer {
    textures: HashMap<(String, u32), TextureHandle>,
    svg_data: HashMap<String, String>,
    current_size: u32,
}

impl PieceRenderer {
    pub fn new() -> Self {
        let mut svg_data = HashMap::new();
        for (key, data) in PIECE_SVGS {
            svg_data.insert(key.to_string(), data.to_string());
        }

        Self {
            textures: HashMap::new(),
            svg_data,
            current_size: 0,
        }
    }

    pub fn get_texture(
        &mut self,
        ctx: &Context,
        role: Role,
        color: Color,
        size: u32,
    ) -> Option<&TextureHandle> {
        let key = piece_key(role, color).to_string();
        let cache_key = (key.clone(), size);

        if !self.textures.contains_key(&cache_key) {
            if let Some(svg_str) = self.svg_data.get(&key) {
                if let Some(image) = self.render_svg(svg_str, size) {
                    let texture = ctx.load_texture(
                        format!("piece_{}_{}", key, size),
                        image,
                        TextureOptions::LINEAR,
                    );
                    self.textures.insert(cache_key.clone(), texture);
                }
            }
        }

        self.textures.get(&cache_key)
    }

    fn render_svg(&self, svg_str: &str, size: u32) -> Option<ColorImage> {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_str(svg_str, &opt).ok()?;

        let fit_to = tiny_skia::Size::from_wh(size as f32, size as f32)?;
        let sx = fit_to.width() / tree.size().width();
        let sy = fit_to.height() / tree.size().height();
        let transform = tiny_skia::Transform::from_scale(sx, sy);

        let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        let pixels: Vec<Color32> = pixmap
            .data()
            .chunks(4)
            .map(|chunk| Color32::from_rgba_unmultiplied(chunk[0], chunk[1], chunk[2], chunk[3]))
            .collect();

        Some(ColorImage {
            size: [size as usize, size as usize],
            pixels,
            source_size: vec2(size as f32, size as f32),
        })
    }

    pub fn invalidate_cache(&mut self) {
        self.textures.clear();
    }

    pub fn set_size(&mut self, size: u32) {
        if self.current_size != size {
            self.current_size = size;
        }
    }
}

impl Default for PieceRenderer {
    fn default() -> Self {
        Self::new()
    }
}
