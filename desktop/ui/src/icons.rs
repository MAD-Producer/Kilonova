//! Shared, themeable SVG icon registry exported from the current Ardot design.
//!
//! Canonical SVGs live in `icons/`; their checked generated path data is compiled
//! once into meshes. Drawing only transforms cached vertices in logical pixels.
use crate::{
    paint::{PaintList, Shape},
    style::{State, States, StyleSheet},
    Color, Input, Rect,
};
use lyon_tessellation::{
    math::point, path::Path, BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex,
    LineCap, LineJoin, StrokeOptions, StrokeTessellator, StrokeVertex, VertexBuffers,
};
use serde::Deserialize;
use std::{collections::HashMap, sync::OnceLock};

macro_rules! icons {
    ($($variant:ident => $id:literal),+ $(,)?) => {
        /// Stable IDs shared by toolbar, timeline, panel and node views.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($variant),+ }
        impl Icon {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub const fn id(self) -> &'static str { match self { $(Self::$variant => $id),+ } }
            pub fn from_id(id: &str) -> Option<Self> { match id { $($id => Some(Self::$variant)),+, _ => None } }
        }
    };
}
icons! {
    Select => "select", Hand => "hand", Move => "move", Pen => "pen", Text => "text",
    Shape => "shape", Camera => "camera", Curve => "curve", Folder => "folder", Cloud => "cloud",
    Image => "image", Video => "video", Audio => "audio", Composition => "composition",
    Node => "node", Fx => "fx", Play => "play", Pause => "pause", Previous => "previous",
    Next => "next", Volume => "volume", Muted => "muted", Expand => "expand", Dock => "dock",
    Search => "search", More => "more", Settings => "settings", Star => "star", Eye => "eye",
    Lock => "lock", Link => "link", Key => "key", Plus => "plus", Close => "x",
    Export => "export", Download => "download", Check => "check", Compile => "compile",
    Undo => "undo", Redo => "redo",
    Stopwatch => "stopwatch",
    Menu => "menu",
    Rotate => "rotate", Scale => "scale", Anchor => "anchor",
    Reset => "reset",
    ChevronUp => "chevron_up", ChevronDown => "chevron_down", ChevronRight => "chevron_right",
    Minimize => "minimize", Maximize => "maximize",
    AlignLeft => "align_left", AlignCenter => "align_center", AlignRight => "align_right",
}
#[derive(Deserialize)]
struct Source {
    shapes: Vec<Primitive>,
}
#[derive(Deserialize)]
struct Primitive {
    commands: Vec<Vec<serde_json::Value>>,
    fill: bool,
    stroke: f32,
    cap: String,
    join: String,
}
type Mesh = Vec<[[f32; 2]; 3]>;
static MESHES: OnceLock<HashMap<String, Mesh>> = OnceLock::new();
fn meshes() -> &'static HashMap<String, Mesh> {
    MESHES.get_or_init(|| {
        let sources: HashMap<String, Source> =
            serde_json::from_str(include_str!("../icons/paths.json"))
                .expect("checked SVG path data");
        sources
            .into_iter()
            .map(|(id, source)| (id, compile(source)))
            .collect()
    })
}
fn compile(source: Source) -> Mesh {
    let mut mesh = vec![];
    for primitive in source.shapes {
        let mut builder = Path::builder();
        let mut open = false;
        for command in primitive.commands {
            let p = |i: usize| {
                point(
                    command[i].as_f64().unwrap() as f32,
                    command[i + 1].as_f64().unwrap() as f32,
                )
            };
            match command[0].as_str().unwrap() {
                "M" => {
                    if open {
                        builder.end(false);
                    }
                    builder.begin(p(1));
                    open = true;
                }
                "L" => {
                    builder.line_to(p(1));
                }
                "Q" => {
                    builder.quadratic_bezier_to(p(1), p(3));
                }
                "C" => {
                    builder.cubic_bezier_to(p(1), p(3), p(5));
                }
                "Z" => {
                    builder.end(true);
                    open = false;
                }
                _ => unreachable!("generator validates SVG commands"),
            }
        }
        if open {
            builder.end(false);
        }
        let path = builder.build();
        let mut geometry = VertexBuffers::<[f32; 2], u32>::new();
        if primitive.fill {
            FillTessellator::new()
                .tessellate_path(
                    &path,
                    &FillOptions::default()
                        .with_fill_rule(FillRule::NonZero)
                        .with_tolerance(0.015),
                    &mut BuffersBuilder::new(&mut geometry, |v: FillVertex<'_>| {
                        v.position().to_array()
                    }),
                )
                .expect("bundled SVG fill");
        }
        if primitive.stroke > 0. {
            let cap = match primitive.cap.as_str() {
                "round" => LineCap::Round,
                "square" => LineCap::Square,
                "butt" => LineCap::Butt,
                _ => panic!("unsupported SVG cap"),
            };
            let join = match primitive.join.as_str() {
                "round" => LineJoin::Round,
                "bevel" => LineJoin::Bevel,
                "miter" => LineJoin::Miter,
                _ => panic!("unsupported SVG join"),
            };
            StrokeTessellator::new()
                .tessellate_path(
                    &path,
                    &StrokeOptions::default()
                        .with_line_width(primitive.stroke)
                        .with_line_cap(cap)
                        .with_line_join(join)
                        .with_tolerance(0.015),
                    &mut BuffersBuilder::new(&mut geometry, |v: StrokeVertex<'_, '_>| {
                        v.position().to_array()
                    }),
                )
                .expect("bundled SVG stroke");
        }
        for indices in geometry.indices.chunks_exact(3) {
            mesh.push([
                geometry.vertices[indices[0] as usize],
                geometry.vertices[indices[1] as usize],
                geometry.vertices[indices[2] as usize],
            ]);
        }
    }
    mesh
}

/// Fit a square icon inside `area`, preserving its original aspect and line width.
pub fn draw(list: &mut PaintList, area: Rect, icon: Icon, size: f32, color: Color) {
    if !size.is_finite()
        || !area.width().is_finite()
        || !area.height().is_finite()
        || color.0[3] <= 0.
    {
        return;
    }
    let edge = size.min(area.width()).min(area.height());
    if edge <= 0. {
        return;
    }
    let scale = edge / 28.;
    let origin = [
        area.min[0] + (area.width() - edge) * 0.5,
        area.min[1] + (area.height() - edge) * 0.5,
    ];
    let points = meshes()[icon.id()]
        .iter()
        .flat_map(|t| {
            t.iter()
                .map(|p| [origin[0] + p[0] * scale, origin[1] + p[1] * scale])
        })
        .collect();
    let mut content = PaintList::default();
    content.push_shape(Shape::Triangles { points }, color);
    // SVG viewport clipping and host clipping must not distort crossing triangles.
    content.clip_to(Rect::new(origin[0], origin[1], edge, edge));
    list.append(content);
}

/// Shared style-state rendering; dispatch and pointer capture remain with widgets.
pub fn button(
    list: &mut PaintList,
    area: Rect,
    icon: Icon,
    input: &Input,
    sheet: &StyleSheet,
    mut states: States,
) -> crate::ui::Response {
    let enabled = !states.contains(State::Disabled);
    let hovered = enabled && area.contains(input.mouse);
    let clicked = hovered
        && matches!(input.pressed, Some((crate::input::MouseButton::Left, p)) if area.contains(p));
    if hovered {
        states = states.with(State::Hovered);
    }
    if hovered && input.held == Some(crate::input::MouseButton::Left) {
        states = states.with(State::Pressed);
    }
    let style = sheet
        .resolve("button.icon", states)
        .unwrap_or_else(|| sheet.resolve("button", states).unwrap());
    crate::style::box_style(list, area, style);
    draw(
        list,
        area,
        icon,
        (area.width().min(area.height()) - 4.).min(20.),
        style.foreground,
    );
    crate::ui::Response {
        hovered,
        clicked,
        active: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_covers_all_bundled_sources_and_compiles_finite_meshes() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../icons/manifest.json")).unwrap();
        assert_eq!(manifest["icons"].as_array().unwrap().len(), Icon::ALL.len());
        assert_eq!(meshes().len(), Icon::ALL.len());
        for icon in Icon::ALL {
            assert_eq!(Icon::from_id(icon.id()), Some(*icon));
            let mesh = &meshes()[icon.id()];
            assert!(!mesh.is_empty(), "{}", icon.id());
            assert!(mesh.iter().flatten().flatten().all(|v| v.is_finite()));
        }
        assert_eq!(Icon::from_id("unregistered"), None);
    }
    #[test]
    fn icons_preserve_aspect_clip_viewport_and_share_disabled_style() {
        let mut list = PaintList::default();
        draw(
            &mut list,
            Rect::new(0., 0., 40., 20.),
            Icon::Link,
            28.,
            Color::grey(220),
        );
        let Shape::Triangles { points } = &list.shapes[0] else {
            panic!()
        };
        assert!(points
            .iter()
            .all(|p| p[0] >= 10. && p[0] <= 30. && p[1] >= 0. && p[1] <= 20.));
        let r = button(
            &mut list,
            Rect::new(0., 0., 24., 24.),
            Icon::Play,
            &Input {
                mouse: [12., 12.],
                pressed: Some((crate::input::MouseButton::Left, [12., 12.])),
                ..Default::default()
            },
            crate::style::default_stylesheet(),
            States::default().with(State::Disabled),
        );
        assert!(!r.clicked && !r.hovered);
    }
}
