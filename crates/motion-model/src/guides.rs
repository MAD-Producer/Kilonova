//! Composition-owned editor guides. They are not renderable scene objects.
use crate::{ensure, Result};
use serde::{de::{self, MapAccess, SeqAccess, Visitor}, Deserialize, Deserializer, Serialize};
use std::collections::HashSet;
use std::fmt;

pub const MAX_GUIDES: usize = 512;
pub const MAX_GUIDE_POSITION: f64 = 1_000_000_000.;

/// The coordinate held constant by the line: X is vertical, Y horizontal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", try_from = "String")]
pub enum GuideAxis {
    X,
    Y,
}
impl TryFrom<String> for GuideAxis {
    type Error = &'static str;
    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        match value.as_str() {
            "x" => Ok(Self::X),
            "y" => Ok(Self::Y),
            _ => Err("guide axis must be x or y"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Guide {
    pub id: u64,
    pub axis: GuideAxis,
    /// Composition pixels, independent of viewer scale or native-window DPI.
    pub position: f64,
}

// Keep diagnostics useful without echoing arbitrary strings or unknown keys
// from an imported document/API request. Numeric causes are validated below.
impl<'de> Deserialize<'de> for Guide {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct GuideVisitor;
        impl<'de> Visitor<'de> for GuideVisitor {
            type Value = Guide;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result { f.write_str("a guide object") }
            fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<Guide, E> {
                Err(E::custom("guide must be an object"))
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> std::result::Result<Guide, M::Error> {
                let (mut id, mut axis, mut position) = (None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => {
                            if id.is_some() { return Err(de::Error::duplicate_field("id")); }
                            let value = map.next_value::<serde_json::Value>()?;
                            id = Some(value.as_u64().ok_or_else(|| de::Error::custom("guide ID must be an unsigned integer"))?);
                        }
                        "axis" => {
                            if axis.is_some() { return Err(de::Error::duplicate_field("axis")); }
                            let value = map.next_value::<serde_json::Value>()?;
                            axis = Some(match value.as_str() {
                                Some("x") => GuideAxis::X,
                                Some("y") => GuideAxis::Y,
                                _ => return Err(de::Error::custom("guide axis must be x or y")),
                            });
                        }
                        "position" => {
                            if position.is_some() { return Err(de::Error::duplicate_field("position")); }
                            let value = map.next_value::<serde_json::Value>()?;
                            position = Some(value.as_f64().ok_or_else(|| de::Error::custom("guide position must be a number"))?);
                        }
                        _ => return Err(de::Error::custom("unknown guide field")),
                    }
                }
                Ok(Guide {
                    id: id.ok_or_else(|| de::Error::missing_field("id"))?,
                    axis: axis.ok_or_else(|| de::Error::missing_field("axis"))?,
                    position: position.ok_or_else(|| de::Error::missing_field("position"))?,
                })
            }
        }
        deserializer.deserialize_any(GuideVisitor)
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Vec<Guide>, D::Error> {
    struct GuidesVisitor;
    impl<'de> Visitor<'de> for GuidesVisitor {
        type Value = Vec<Guide>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result { f.write_str("a guide array") }
        fn visit_str<E: de::Error>(self, _: &str) -> std::result::Result<Vec<Guide>, E> {
            Err(E::custom("guides must be an array"))
        }
        fn visit_seq<S: SeqAccess<'de>>(self, mut seq: S) -> std::result::Result<Vec<Guide>, S::Error> {
            let mut guides = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(MAX_GUIDES));
            while let Some(guide) = seq.next_element()? {
                if guides.len() == MAX_GUIDES {
                    return Err(de::Error::custom("composition guide limit exceeded"));
                }
                guides.push(guide);
            }
            Ok(guides)
        }
    }
    deserializer.deserialize_any(GuidesVisitor)
}

pub fn validate(guides: &[Guide]) -> Result<()> {
    ensure(guides.len() <= MAX_GUIDES, "composition guide limit exceeded")?;
    let mut ids = HashSet::with_capacity(guides.len());
    for (index, guide) in guides.iter().enumerate() {
        ensure(guide.id != 0 && ids.insert(guide.id),
            &format!("guide IDs must be unique and nonzero: guide_index={index} id={}", guide.id))?;
        ensure(guide.position.is_finite() && guide.position.abs() <= MAX_GUIDE_POSITION,
            &format!("invalid guide position: guide_index={index} id={} position={}", guide.id, guide.position))?;
    }
    Ok(())
}

pub fn set(project: &mut crate::Project, guides: Vec<Guide>) -> Result<()> {
    validate(&guides)?;
    project.guides = guides;
    Ok(())
}
