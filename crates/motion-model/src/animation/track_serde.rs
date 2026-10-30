//! Decode storage explicitly so preparation failures retain their real cause.
//! An untagged alternative would replace solver errors with a generic mismatch.
use super::*;
use serde::de::{Error as _, MapAccess};
use std::{fmt, marker::PhantomData};

#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "snake_case")]
enum Field {
    Value,
    Keys,
    Axes,
    SpatialAnimation,
    #[serde(other)]
    Unknown,
}

pub(super) struct Visitor<T>(pub(super) PhantomData<T>);
impl<'de, T: Tween + Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
    type Value = Track<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("exactly one ordinary, separated or spatial track store")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> std::result::Result<Track<T>, A::Error> {
        let mut value: Option<T> = None;
        let mut keys: Option<Vec<Keyframe<T>>> = None;
        let mut axes: Option<Box<AxisTracks>> = None;
        let mut spatial: Option<Box<SpatialAnimation<T>>> = None;
        while let Some(field) = map.next_key()? {
            match field {
                Field::Unknown => {
                    return Err(A::Error::custom(
                        "unknown track field; expected value, keys, axes or spatial_animation",
                    ))
                }
                Field::Value => {
                    if value.is_some() {
                        return Err(A::Error::duplicate_field("value"));
                    }
                    value = Some(map.next_value()?);
                }
                Field::Keys => {
                    if keys.is_some() {
                        return Err(A::Error::duplicate_field("keys"));
                    }
                    keys = Some(map.next_value()?);
                }
                Field::Axes => {
                    if axes.is_some() {
                        return Err(A::Error::duplicate_field("axes"));
                    }
                    axes = Some(map.next_value()?);
                }
                Field::SpatialAnimation => {
                    if spatial.is_some() {
                        return Err(A::Error::duplicate_field("spatial_animation"));
                    }
                    // Deserialize directly, retaining bounded streaming key
                    // decoding and the primary preparation/identity failure.
                    spatial = Some(map.next_value()?);
                }
            }
        }
        if let Some(axes) = axes {
            if value.is_some() || keys.is_some() || spatial.is_some() {
                return Err(A::Error::custom(
                    "separated tracks cannot contain another store",
                ));
            }
            let value = T::from_components([axes.x.value, axes.y.value, axes.z.value]).ok_or_else(
                || A::Error::custom("only vector properties can separate dimensions"),
            )?;
            return Ok(Track {
                value,
                keys: Vec::new(),
                axes: Some(axes),
                spatial_animation: None,
            });
        }
        let value = value.ok_or_else(|| A::Error::missing_field("value"))?;
        if let Some(spatial) = spatial {
            if keys.is_some() {
                return Err(A::Error::custom(
                    "spatial animation cannot coexist with ordinary keys",
                ));
            }
            Ok(Track {
                value,
                keys: Vec::new(),
                axes: None,
                spatial_animation: Some(spatial),
            })
        } else {
            let keys = keys.ok_or_else(|| A::Error::missing_field("keys"))?;
            Ok(Track {
                value,
                keys,
                axes: None,
                spatial_animation: None,
            })
        }
    }
}
