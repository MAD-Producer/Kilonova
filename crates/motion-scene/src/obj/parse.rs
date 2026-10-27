use super::{
    material::{relative_path, Libraries},
    polygon, text, Handedness, ObjMetadata, ObjOptions, PrimitiveInfo, UpAxis,
};
use crate::{
    accessors::Budget, invalid, unsupported, AssetResolver, ImportLimits, Mesh, Node, Primitive,
    Result, Scene, SourceFormat,
};
use glam::{DVec3, Mat4};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Corner {
    v: usize,
    uv: Option<usize>,
    normal: Option<usize>,
}
#[derive(Clone, PartialEq, Eq)]
struct State {
    object: usize,
    name: String,
    groups: Vec<String>,
    material: Option<String>,
    smooth: Option<u32>,
}
struct Block {
    state: State,
    line: usize,
    corners: Vec<Corner>,
    indices: Vec<usize>,
    uv: bool,
    normal: bool,
    color: bool,
}

pub(super) fn load(
    source: &[u8],
    resolver: &mut impl AssetResolver,
    options: &ObjOptions,
    limits: &ImportLimits,
) -> Result<Scene> {
    if source.len() > limits.max_source_bytes {
        return Err(invalid("obj", "source", "source byte limit exceeded"));
    }
    if !options.metres_per_unit.is_finite() || options.metres_per_unit <= 0. {
        return Err(invalid(
            "obj",
            "options",
            "metres per unit must be finite and positive",
        ));
    }
    if limits.max_node_depth == 0 {
        return Err(invalid("obj", "nodes", "node depth limit exceeded"));
    }
    let mut budget = Budget {
        bytes: 0,
        vertices: 0,
        indices: 0,
    };
    let mut libraries = Libraries::new();
    let mut positions = Vec::<[f32; 3]>::new();
    let mut colors = Vec::<Option<[f32; 4]>>::new();
    let mut normals = Vec::<[f32; 3]>::new();
    let mut uvs = Vec::<[f32; 2]>::new();
    let mut state = State {
        object: 0,
        name: String::new(),
        groups: Vec::new(),
        material: None,
        smooth: None,
    };
    let mut object_count = 0usize;
    let mut blocks = Vec::<Block>::new();
    let mut smooth = BTreeMap::<(usize, u32, usize), DVec3>::new();
    let mut work = 0;
    let mut index_count = 0usize;
    let mut corner_count = 0usize;
    let mut nonplanar = 0;
    text::visit(source, "obj", |line, words| {
        let args = &words[1..];
        match words[0].as_str() {
            "v" => {
                if !matches!(args.len(), 3 | 4 | 6) {
                    return Err(unsupported(
                        "obj",
                        line,
                        "vertex requires XYZ, XYZW, or XYZ+RGB",
                    ));
                }
                if positions.len() >= limits.max_vertices {
                    return Err(invalid("obj", line, "vertex count limit exceeded"));
                }
                let mut value = vector(&args[..3], line)?;
                if args.len() == 4 {
                    let w = text::number(&args[3], "obj", line)?;
                    if w == 0. {
                        return Err(invalid("obj", line, "homogeneous vertex has zero W"));
                    }
                    value = value.map(|n| n / w);
                }
                value = convert(value, options, true, line)?;
                let color = if args.len() == 6 {
                    let rgb = vector(&args[3..], line)?;
                    if rgb.iter().any(|v| !(0. ..=1.).contains(v)) {
                        return Err(invalid("obj", line, "vertex color outside unit range"));
                    }
                    Some([rgb[0], rgb[1], rgb[2], 1.])
                } else {
                    None
                };
                positions.push(value);
                colors.push(color);
            }
            "vn" => {
                if args.len() != 3 {
                    return Err(invalid("obj", line, "normal requires three components"));
                }
                if normals.len() >= limits.max_vertices {
                    return Err(invalid("obj", line, "normal count limit exceeded"));
                }
                let n = convert(vector(args, line)?, options, false, line)?;
                let n = DVec3::new(n[0] as f64, n[1] as f64, n[2] as f64);
                if n.length_squared() == 0. {
                    return Err(invalid("obj", line, "zero-length normal"));
                }
                normals.push(n.normalize().as_vec3().to_array());
            }
            "vt" => {
                if !(1..=3).contains(&args.len()) {
                    return Err(invalid(
                        "obj",
                        line,
                        "texture coordinate requires one to three components",
                    ));
                }
                if uvs.len() >= limits.max_vertices {
                    return Err(invalid("obj", line, "UV count limit exceeded"));
                }
                if args.len() == 3 && text::number(&args[2], "obj", line)? != 0. {
                    return Err(unsupported(
                        "obj",
                        line,
                        "3D texture coordinates are unsupported",
                    ));
                }
                let u = text::number(&args[0], "obj", line)?;
                let mut v = if args.len() > 1 {
                    text::number(&args[1], "obj", line)?
                } else {
                    0.
                };
                if options.flip_v {
                    v = 1. - v;
                }
                if !v.is_finite() {
                    return Err(invalid("obj", line, "UV conversion overflow"));
                }
                uvs.push([u, v]);
            }
            "o" => {
                object_count = object_count
                    .checked_add(1)
                    .filter(|&n| n <= limits.max_items)
                    .ok_or_else(|| invalid("obj", line, "object count limit exceeded"))?;
                state.object = object_count;
                state.name = text::name(args, "obj", line)?;
            }
            "g" => {
                if args.len() > 32 {
                    return Err(invalid("obj", line, "group membership limit exceeded"));
                }
                state.groups = args
                    .iter()
                    .filter(|s| s.as_str() != "off")
                    .map(|name| text::name(std::slice::from_ref(name), "obj", line))
                    .collect::<Result<_>>()?;
            }
            "usemtl" => state.material = Some(text::name(args, "obj", line)?),
            "mtllib" => {
                if args.is_empty() {
                    return Err(invalid("obj", line, "empty material library declaration"));
                }
                for name in args {
                    let path = relative_path("", name, line)?;
                    libraries.load(&path, resolver, options, limits, &mut budget)?;
                }
            }
            "s" => {
                if args.len() != 1 {
                    return Err(invalid("obj", line, "expected one smoothing group"));
                }
                state.smooth = match args[0].as_str() {
                    "off" | "0" => None,
                    "on" => Some(1),
                    value => Some(
                        value
                            .parse::<u32>()
                            .ok()
                            .filter(|&n| n > 0)
                            .ok_or_else(|| invalid("obj", line, "invalid smoothing group"))?,
                    ),
                };
            }
            "f" => {
                if !(3..=polygon::MAX_CORNERS).contains(&args.len()) {
                    return Err(invalid(
                        "obj",
                        line,
                        "polygon corner count outside supported range",
                    ));
                }
                let mut face = args
                    .iter()
                    .map(|s| corner(s, positions.len(), uvs.len(), normals.len(), line))
                    .collect::<Result<Vec<_>>>()?;
                if options.handedness == Handedness::Left {
                    face.reverse();
                }
                let has_uv = face[0].uv.is_some();
                let has_normal = face[0].normal.is_some();
                let has_color = colors[face[0].v].is_some();
                if face.iter().any(|c| {
                    c.uv.is_some() != has_uv
                        || c.normal.is_some() != has_normal
                        || colors[c.v].is_some() != has_color
                }) {
                    return Err(invalid("obj", line, "inconsistent face attribute layout"));
                }
                let points: Vec<_> = face.iter().map(|c| positions[c.v]).collect();
                let polygon = polygon::triangulate(&points, line, &mut work)?;
                nonplanar += usize::from(polygon.nonplanar);
                index_count = index_count
                    .checked_add(polygon.triangles.len() * 3)
                    .filter(|&n| n <= limits.max_indices)
                    .ok_or_else(|| invalid("obj", line, "index count limit exceeded"))?;
                corner_count = corner_count
                    .checked_add(face.len())
                    .filter(|&n| n <= limits.max_indices)
                    .ok_or_else(|| invalid("obj", line, "face corner count limit exceeded"))?;
                if let Some(group) = state.smooth {
                    // Smooth across UV seams and material splits, but never across objects/groups.
                    for c in &face {
                        *smooth.entry((state.object, group, c.v)).or_default() +=
                            polygon.area_normal;
                    }
                }
                let append = blocks.last().is_some_and(|b| {
                    b.state == state
                        && b.uv == has_uv
                        && b.normal == has_normal
                        && b.color == has_color
                });
                if !append {
                    if blocks.len() >= limits.max_items {
                        return Err(invalid("obj", line, "primitive count limit exceeded"));
                    }
                    blocks.push(Block {
                        state: state.clone(),
                        line,
                        corners: Vec::new(),
                        indices: Vec::new(),
                        uv: has_uv,
                        normal: has_normal,
                        color: has_color,
                    });
                }
                let b = blocks
                    .last_mut()
                    .ok_or_else(|| invalid("obj", line, "missing face block"))?;
                let offset = b.corners.len();
                for triangle in polygon.triangles {
                    b.indices.extend(triangle.map(|i| i + offset));
                }
                b.corners.extend(face);
            }
            _ => {
                return Err(unsupported(
                    "obj",
                    line,
                    "unsupported OBJ directive (no scripts, lines or freeform surfaces)",
                ))
            }
        }
        Ok(())
    })?;
    if blocks.is_empty() {
        return Err(invalid("obj", "meshes", "source contains no polygon faces"));
    }
    let mut meshes = Vec::<Mesh>::new();
    let mut object_meshes = BTreeMap::new();
    let mut info = Vec::new();
    for block in blocks {
        let material = block
            .state
            .material
            .as_ref()
            .map(|name| {
                libraries.names.get(name).copied().ok_or_else(|| {
                    invalid("obj", block.line, "face references an undefined material")
                })
            })
            .transpose()?;
        let normals_present = block.normal || block.state.smooth.is_some();
        let mut primitive = Primitive {
            positions: Vec::new(),
            normals: normals_present.then(Vec::new),
            tangents: None,
            texcoords: [block.uv.then(Vec::new), None],
            colors: block.color.then(Vec::new),
            indices: Vec::new(),
            material,
        };
        let mut unique = BTreeMap::<Corner, u32>::new();
        budget.indices = budget
            .indices
            .checked_add(block.indices.len())
            .filter(|&n| n <= limits.max_indices)
            .ok_or_else(|| invalid("obj", block.line, "decoded index limit exceeded"))?;
        budget.reserve(block.indices.len(), 4, limits)?;
        for index in block.indices {
            let c = block.corners[index];
            let id = if let Some(&id) = unique.get(&c) {
                id
            } else {
                budget.vertices = budget
                    .vertices
                    .checked_add(1)
                    .filter(|&n| n <= limits.max_vertices)
                    .ok_or_else(|| invalid("obj", block.line, "decoded vertex limit exceeded"))?;
                budget.reserve(
                    1,
                    12 + usize::from(normals_present) * 12
                        + usize::from(block.uv) * 8
                        + usize::from(block.color) * 16,
                    limits,
                )?;
                let id = u32::try_from(primitive.positions.len())
                    .map_err(|_| invalid("obj", block.line, "vertex index exceeds u32"))?;
                primitive.positions.push(positions[c.v]);
                if let Some(dest) = &mut primitive.normals {
                    let normal = if let Some(n) = c.normal {
                        normals[n]
                    } else {
                        let n = smooth
                            .get(&(block.state.object, block.state.smooth.unwrap_or(0), c.v))
                            .copied()
                            .unwrap_or(DVec3::ZERO);
                        if n.length_squared() == 0. {
                            return Err(invalid(
                                "obj",
                                block.line,
                                "smoothing normals cancel to zero",
                            ));
                        }
                        n.normalize().as_vec3().to_array()
                    };
                    dest.push(normal);
                }
                if let Some(dest) = &mut primitive.texcoords[0] {
                    dest.push(
                        uvs[c
                            .uv
                            .ok_or_else(|| invalid("obj", block.line, "missing corner UV"))?],
                    );
                }
                if let Some(dest) = &mut primitive.colors {
                    dest.push(
                        colors[c.v]
                            .ok_or_else(|| invalid("obj", block.line, "missing corner color"))?,
                    );
                }
                unique.insert(c, id);
                id
            };
            primitive.indices.push(id);
        }
        let mesh = *object_meshes.entry(block.state.object).or_insert_with(|| {
            meshes.push(Mesh {
                primitives: Vec::new(),
            });
            meshes.len() - 1
        });
        info.push(PrimitiveInfo {
            mesh,
            primitive: meshes[mesh].primitives.len(),
            object: block.state.name,
            groups: block.state.groups,
            smoothing_group: block.state.smooth,
            first_line: block.line,
        });
        meshes[mesh].primitives.push(primitive);
    }
    if meshes.len() > limits.max_items {
        return Err(invalid("obj", "meshes", "mesh count limit exceeded"));
    }
    let nodes = (0..meshes.len())
        .map(|id| Node {
            local: Mat4::IDENTITY.to_cols_array(),
            mesh: Some(id),
            children: Vec::new(),
            rest: None,
        })
        .collect();
    Ok(Scene {
        format: SourceFormat::Obj,
        obj_metadata: Some(ObjMetadata {
            options: options.clone(),
            primitives: info,
            projected_nonplanar_faces: nonplanar,
        }),
        extra_resources: libraries.files,
        source_sha256: Sha256::digest(source).into(),
        buffers: Vec::new(),
        images: libraries.images,
        textures: libraries.textures,
        samplers: libraries.samplers,
        materials: libraries.materials,
        scenes: vec![(0..meshes.len()).collect()],
        default_scene: Some(0),
        meshes,
        nodes,
        animations: Vec::new(),
        ignored_optional_extensions: Vec::new(),
        retained_bytes: budget.bytes,
    })
}
fn vector(args: &[String], line: usize) -> Result<[f32; 3]> {
    Ok([
        text::number(&args[0], "obj", line)?,
        text::number(&args[1], "obj", line)?,
        text::number(&args[2], "obj", line)?,
    ])
}
fn convert(v: [f32; 3], options: &ObjOptions, position: bool, line: usize) -> Result<[f32; 3]> {
    let mut v = v.map(|n| n as f64);
    if options.handedness == Handedness::Left {
        v[2] = -v[2];
    }
    if options.up_axis == UpAxis::Z {
        v = [v[0], v[2], -v[1]];
    }
    if position {
        v = v.map(|n| n * options.metres_per_unit as f64);
    }
    let out = v.map(|n| n as f32);
    if out.iter().any(|v| !v.is_finite()) {
        return Err(invalid("obj", line, "coordinate conversion overflow"));
    }
    Ok(out)
}
fn corner(s: &str, vertices: usize, uvs: usize, normals: usize, line: usize) -> Result<Corner> {
    let parts: Vec<_> = s.split('/').collect();
    if parts.len() > 3
        || parts[0].is_empty()
        || (parts.len() == 2 && parts[1].is_empty())
        || (parts.len() == 3 && parts[2].is_empty())
    {
        return Err(invalid("obj", line, "malformed face corner"));
    }
    Ok(Corner {
        v: index(parts[0], vertices, line)?,
        uv: parts
            .get(1)
            .filter(|s| !s.is_empty())
            .map(|s| index(s, uvs, line))
            .transpose()?,
        normal: parts.get(2).map(|s| index(s, normals, line)).transpose()?,
    })
}
fn index(value: &str, count: usize, line: usize) -> Result<usize> {
    let n = value
        .parse::<i64>()
        .map_err(|_| invalid("obj", line, "invalid corner index"))?;
    let i = if n > 0 {
        n - 1
    } else if n < 0 {
        (count as i64).checked_add(n).unwrap_or(-1)
    } else {
        -1
    };
    usize::try_from(i)
        .ok()
        .filter(|&i| i < count)
        .ok_or_else(|| invalid("obj", line, "corner index outside defined attribute range"))
}
