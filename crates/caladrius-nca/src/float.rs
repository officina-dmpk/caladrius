//! Serde helpers for numbers that may be missing or non-finite in the input (golden rule 4: the
//! CLI and the MCP server send JSON, which has no NaN or infinity).
//!
//! Written: a finite number as a number; NaN as `null` in a concentration list (a missing value,
//! NCA-DAT-03) and as `"NaN"` elsewhere; infinities as `"inf"` and `"-inf"`.
//! Read: a number, `null` (NaN), or one of the texts `"NaN"`, `"inf"`, `"+inf"`, `"-inf"`,
//! `"infinity"`, `"-infinity"` (any case).

use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

/// One number as written.
#[derive(Serialize)]
#[serde(untagged)]
enum Out {
    Number(f64),
    Text(&'static str),
    Missing(Option<f64>),
}

fn out(x: f64, nan_as_null: bool) -> Out {
    if x.is_finite() {
        Out::Number(x)
    } else if x.is_nan() {
        if nan_as_null {
            Out::Missing(None)
        } else {
            Out::Text("NaN")
        }
    } else if x > 0.0 {
        Out::Text("inf")
    } else {
        Out::Text("-inf")
    }
}

/// One number as read.
#[derive(Deserialize)]
#[serde(untagged)]
enum In {
    Number(f64),
    Text(String),
}

fn read<E: de::Error>(value: Option<In>) -> Result<f64, E> {
    match value {
        None => Ok(f64::NAN),
        Some(In::Number(x)) => Ok(x),
        Some(In::Text(text)) => match text.to_ascii_lowercase().as_str() {
            "nan" => Ok(f64::NAN),
            "inf" | "+inf" | "infinity" | "+infinity" => Ok(f64::INFINITY),
            "-inf" | "-infinity" => Ok(f64::NEG_INFINITY),
            _ => Err(E::custom(format!(
                "{text:?} is not a number; write a number, null for a missing value, \"NaN\", \"inf\" or \"-inf\""
            ))),
        },
    }
}

/// A scalar: non-finite values as text.
pub(crate) fn ser<S: Serializer>(x: &f64, s: S) -> Result<S::Ok, S::Error> {
    out(*x, false).serialize(s)
}

/// A scalar: number, null or text.
pub(crate) fn de<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    read(Option::<In>::deserialize(d)?)
}

/// A list of values where NaN means missing: NaN as `null`.
pub(crate) fn ser_vec<S: Serializer>(v: &[f64], s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(v.iter().map(|x| out(*x, true)))
}

/// A list of numbers, nulls or texts.
pub(crate) fn de_vec<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<f64>, D::Error> {
    Vec::<Option<In>>::deserialize(d)?
        .into_iter()
        .map(read)
        .collect()
}
