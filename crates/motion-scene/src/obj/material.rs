use super::{text, MapKind, MaterialColorSpace, ObjOptions, WavefrontMap, WavefrontMaterial};
use crate::{
    accessors::Budget, import::image_mime, invalid, resources::load_path, unsupported, AlphaMode,
    AssetResolver, Image, ImportLimits, Material, MaterialModel, Resource, ResourceKind, Result,
    Sampler, Texture,
};
use std::collections::BTreeMap;

pub(super) struct Libraries {
    pub materials: Vec<Material>,
    pub names: BTreeMap<String, usize>,
    pub images: Vec<Image>,
    pub textures: Vec<Texture>,
    pub samplers: Vec<Sampler>,
    pub files: Vec<Resource>,
    images_by_path: BTreeMap<String, usize>,
}
impl Libraries {
    pub fn new() -> Self {
        Self {
            materials: Vec::new(),
            names: BTreeMap::new(),
            images: Vec::new(),
            textures: Vec::new(),
            samplers: Vec::new(),
            files: Vec::new(),
            images_by_path: BTreeMap::new(),
        }
    }
    pub fn load(
        &mut self,
        path: &str,
        resolver: &mut impl AssetResolver,
        options: &ObjOptions,
        limits: &ImportLimits,
        budget: &mut Budget,
    ) -> Result<()> {
        if self
            .files
            .iter()
            .any(|r| r.locator.as_deref() == Some(path))
        {
            return Ok(());
        }
        if self.files.len() >= limits.max_items {
            return Err(invalid("mtl", "libraries", "library count limit exceeded"));
        }
        let file = load_path(
            path,
            ResourceKind::MaterialLibrary,
            limits
                .max_resource_bytes
                .min(limits.max_json_bytes)
                .min(limits.max_retained_bytes.saturating_sub(budget.bytes)),
            resolver,
        )?;
        budget.reserve(file.bytes.len(), 1, limits)?;
        let mut current: Option<WavefrontMaterial> = None;
        text::visit(&file.bytes, "mtl", |line, words| {
            let args = &words[1..];
            if words[0] == "newmtl" {
                if let Some(m) = current.take() {
                    self.store(m, limits, line)?;
                }
                current = Some(WavefrontMaterial {
                    name: text::name(args, "mtl", line)?,
                    color_space: options.material_color_space,
                    ambient: None,
                    diffuse: None,
                    specular: None,
                    emissive: None,
                    transmission: None,
                    specular_exponent: None,
                    optical_density: None,
                    dissolve: None,
                    dissolve_halo: false,
                    illumination: None,
                    maps: BTreeMap::new(),
                });
                return Ok(());
            }
            let m = current
                .as_mut()
                .ok_or_else(|| invalid("mtl", line, "material property precedes newmtl"))?;
            match words[0].as_str() {
                "Ka" | "Kd" | "Ks" | "Ke" | "Tf" => {
                    if args.len() != 3 {
                        return Err(unsupported(
                            "mtl",
                            line,
                            "only RGB material colors are supported",
                        ));
                    }
                    let mut rgb = [0.; 3];
                    for (v, s) in rgb.iter_mut().zip(args) {
                        *v = text::number(s, "mtl", line)?;
                        if *v < 0. || (words[0] != "Ke" && *v > 1.) {
                            return Err(invalid(
                                "mtl",
                                line,
                                "color component outside supported range",
                            ));
                        }
                    }
                    match words[0].as_str() {
                        "Ka" => m.ambient = Some(rgb),
                        "Kd" => m.diffuse = Some(rgb),
                        "Ks" => m.specular = Some(rgb),
                        "Ke" => m.emissive = Some(rgb),
                        _ => m.transmission = Some(rgb),
                    }
                }
                "Ns" | "Ni" | "d" | "Tr" => {
                    let (halo, values) =
                        if words[0] == "d" && args.first().is_some_and(|a| a == "-halo") {
                            (true, &args[1..])
                        } else {
                            (false, args)
                        };
                    if values.len() != 1 {
                        return Err(invalid("mtl", line, "expected one scalar material value"));
                    }
                    let value = text::number(&values[0], "mtl", line)?;
                    let range = match words[0].as_str() {
                        "Ns" => 0. ..=1000.,
                        "Ni" => 0.001..=10.,
                        _ => 0. ..=1.,
                    };
                    if !range.contains(&value) {
                        return Err(invalid(
                            "mtl",
                            line,
                            "scalar material value outside supported range",
                        ));
                    }
                    match words[0].as_str() {
                        "Ns" => m.specular_exponent = Some(value),
                        "Ni" => m.optical_density = Some(value),
                        _ => {
                            let alpha = if words[0] == "Tr" { 1. - value } else { value };
                            if m.dissolve.is_some_and(|old| (old - alpha).abs() > 1e-6) {
                                return Err(invalid(
                                    "mtl",
                                    line,
                                    "conflicting dissolve declarations",
                                ));
                            }
                            m.dissolve = Some(alpha);
                            m.dissolve_halo |= halo;
                        }
                    }
                }
                "illum" => {
                    if args.len() != 1 {
                        return Err(invalid("mtl", line, "expected one illumination model"));
                    }
                    m.illumination = Some(
                        args[0]
                            .parse::<u8>()
                            .ok()
                            .filter(|n| *n <= 10)
                            .ok_or_else(|| invalid("mtl", line, "invalid illumination model"))?,
                    );
                }
                keyword => {
                    let kind = match keyword {
                        "map_Ka" => MapKind::Ambient,
                        "map_Kd" => MapKind::Diffuse,
                        "map_Ks" => MapKind::Specular,
                        "map_Ke" => MapKind::Emissive,
                        "map_d" => MapKind::Opacity,
                        "map_Ns" => MapKind::Exponent,
                        "bump" | "map_bump" => MapKind::Bump,
                        "norm" => MapKind::Normal,
                        "disp" => MapKind::Displacement,
                        _ => {
                            return Err(unsupported("mtl", line, "unrecognized material directive"))
                        }
                    };
                    if m.maps.contains_key(&kind) {
                        return Err(invalid("mtl", line, "duplicate material map"));
                    }
                    let (mut map, filename) = map_options(args, line)?;
                    let image_path = relative_path(path, &filename, line)?;
                    let image = if let Some(&id) = self.images_by_path.get(&image_path) {
                        id
                    } else {
                        if self.images.len() >= limits.max_items {
                            return Err(invalid("mtl", line, "image count limit exceeded"));
                        }
                        let resource = load_path(
                            &image_path,
                            ResourceKind::Image,
                            limits
                                .max_resource_bytes
                                .min(limits.max_retained_bytes.saturating_sub(budget.bytes)),
                            resolver,
                        )?;
                        budget.reserve(resource.bytes.len(), 1, limits)?;
                        let mime_type = image_mime(self.images.len(), &resource.bytes, None, None)?;
                        let id = self.images.len();
                        self.images.push(Image {
                            resource,
                            mime_type,
                        });
                        self.images_by_path.insert(image_path, id);
                        id
                    };
                    if self.textures.len() >= limits.max_items {
                        return Err(invalid("mtl", line, "texture count limit exceeded"));
                    }
                    // Keep repeat/clamp per map; the same image can have different sampling.
                    let wrap = if map.clamp { 33071 } else { 10497 };
                    let sampler = self.samplers.len();
                    self.samplers.push(Sampler {
                        mag_filter: None,
                        min_filter: None,
                        wrap_s: wrap,
                        wrap_t: wrap,
                    });
                    map.texture = self.textures.len();
                    self.textures.push(Texture {
                        image,
                        sampler: Some(sampler),
                    });
                    m.maps.insert(kind, map);
                }
            }
            Ok(())
        })?;
        if let Some(m) = current {
            self.store(m, limits, 0)?;
        }
        self.files.push(file);
        Ok(())
    }
    fn store(&mut self, m: WavefrontMaterial, limits: &ImportLimits, line: usize) -> Result<()> {
        if self.materials.len() >= limits.max_items {
            return Err(invalid("mtl", line, "material count limit exceeded"));
        }
        if self.names.contains_key(&m.name) {
            return Err(invalid("mtl", line, "duplicate material name"));
        }
        self.names.insert(m.name.clone(), self.materials.len());
        let rgb = linear(m.diffuse.unwrap_or([1.; 3]), m.color_space);
        let emissive = linear(m.emissive.unwrap_or([0.; 3]), m.color_space);
        if rgb.iter().chain(&emissive).any(|value| !value.is_finite()) {
            return Err(invalid("mtl", line, "material color conversion overflow"));
        }
        let alpha = m.dissolve.unwrap_or(1.);
        self.materials.push(Material {
            model: MaterialModel::Wavefront(Box::new(m.clone())),
            base_color: [rgb[0], rgb[1], rgb[2], alpha],
            emissive,
            alpha: if alpha < 1. || m.maps.contains_key(&MapKind::Opacity) {
                AlphaMode::Blend
            } else {
                AlphaMode::Opaque
            },
            // Generic PBR slots remain unset: map transforms and classic lighting are not PBR.
            ..Material::default()
        });
        Ok(())
    }
}

/// Normalize local OBJ names against the containing MTL, without URI decoding.
/// Backslashes are accepted as exporter path separators, never as escapes.
pub(super) fn relative_path(containing: &str, name: &str, line: usize) -> Result<String> {
    if name.len() > 4096
        || name.contains([':', '\0', '?', '#'])
        || name.chars().any(char::is_control)
    {
        return Err(invalid("obj_path", line, "invalid local resource name"));
    }
    let name = name.replace('\\', "/");
    if name.starts_with('/') || name.is_empty() {
        return Err(invalid("obj_path", line, "resource name must be relative"));
    }
    let mut parts: Vec<&str> = containing
        .rsplit_once('/')
        .map_or(Vec::new(), |(dir, _)| dir.split('/').collect());
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(invalid(
                        "obj_path",
                        line,
                        "resource escapes import directory",
                    ));
                }
            }
            _ => parts.push(part),
        }
    }
    let path = parts.join("/");
    crate::resources::relative_path_checked(&path).map_err(|e| invalid("obj_path", line, e))?;
    Ok(path)
}

fn linear(rgb: [f32; 3], space: MaterialColorSpace) -> [f32; 3] {
    if space == MaterialColorSpace::Linear {
        return rgb;
    }
    rgb.map(|v| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    })
}
fn map_options(args: &[String], line: usize) -> Result<(WavefrontMap, String)> {
    let mut map = WavefrontMap {
        texture: 0,
        offset: [0.; 3],
        scale: [1.; 3],
        clamp: false,
        bump_multiplier: 1.,
        channel: None,
    };
    let mut i = 0;
    while i < args.len() && args[i].starts_with('-') {
        let flag = &args[i];
        i += 1;
        match flag.as_str() {
            "-o" | "-s" => {
                let dest = if flag == "-o" {
                    &mut map.offset
                } else {
                    &mut map.scale
                };
                let mut count = 0;
                while count < 3 && i < args.len() && args[i].parse::<f32>().is_ok() {
                    dest[count] = text::number(&args[i], "mtl", line)?;
                    count += 1;
                    i += 1;
                }
                if count == 0 {
                    return Err(invalid("mtl", line, "map vector option has no components"));
                }
            }
            "-clamp" | "-bm" | "-imfchan" => {
                let value = args
                    .get(i)
                    .ok_or_else(|| invalid("mtl", line, "map option has no value"))?;
                i += 1;
                match flag.as_str() {
                    "-clamp" => {
                        map.clamp = match value.as_str() {
                            "on" => true,
                            "off" => false,
                            _ => return Err(invalid("mtl", line, "clamp must be on/off")),
                        }
                    }
                    "-bm" => map.bump_multiplier = text::number(value, "mtl", line)?,
                    _ => {
                        let mut chars = value.chars();
                        map.channel = chars.next().filter(|c| "rgbmlz".contains(*c));
                        if map.channel.is_none() || chars.next().is_some() {
                            return Err(invalid("mtl", line, "unsupported map channel"));
                        }
                    }
                }
            }
            _ => return Err(unsupported("mtl", line, "unsupported texture map option")),
        }
    }
    Ok((map, text::name(&args[i..], "mtl", line)?))
}
