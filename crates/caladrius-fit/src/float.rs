//! Serde helpers for a number that may be NaN or infinite (in an error report or an option): JSON
//! has no such values, so they are written as the texts `"NaN"`, `"inf"`, `"-inf"` and read back,
//! together with plain numbers and `null` (NaN). Copied from `caladrius-models`, where the helper
//! is private to the crate.

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(untagged)]
enum Out {
    Number(f64),
    Text(&'static str),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum In {
    Number(f64),
    Text(String),
}

/// Writes a finite number as a number, otherwise as text.
pub(crate) fn ser<S: Serializer>(x: &f64, s: S) -> Result<S::Ok, S::Error> {
    let out = if x.is_finite() {
        Out::Number(*x)
    } else if x.is_nan() {
        Out::Text("NaN")
    } else if *x > 0.0 {
        Out::Text("inf")
    } else {
        Out::Text("-inf")
    };
    out.serialize(s)
}

/// Reads a number, `null` (NaN) or one of the texts written by [`ser`].
pub(crate) fn de<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    match Option::<In>::deserialize(d)? {
        None => Ok(f64::NAN),
        Some(In::Number(x)) => Ok(x),
        Some(In::Text(text)) => match text.to_ascii_lowercase().as_str() {
            "nan" => Ok(f64::NAN),
            "inf" | "+inf" | "infinity" | "+infinity" => Ok(f64::INFINITY),
            "-inf" | "-infinity" => Ok(f64::NEG_INFINITY),
            _ => Err(de::Error::custom(format!(
                "{text:?} is not a number; write a number, \"NaN\", \"inf\" or \"-inf\""
            ))),
        },
    }
}
