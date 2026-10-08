//! Units of an NCA (`specs/nca.md` NCA-UNIT-01, UNIT-02; task T-025): a small table of time, mass
//! and volume units, the concentration as mass/volume, and the unit of every result. Units are
//! data (golden rule 5): without them the analysis is unitless, as before, and flagged.

use serde::{Deserialize, Serialize};

use crate::NcaError;

/// The units of the input: time, concentration (mass/volume) and dose (mass).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Units {
    /// Time unit of the sampling times: `s`, `min`, `h` or `d`.
    pub time: String,
    /// Concentration unit, mass/volume: e.g. `ng/mL`, `mg/L`, `µg/L`.
    pub concentration: String,
    /// Dose unit, a mass: `ng`, `µg`, `mg` or `g`.
    pub dose: String,
}

/// Mass units with their size in grams.
const MASS: [(&str, f64); 7] = [
    ("ng", 1e-9),
    ("µg", 1e-6),
    ("μg", 1e-6),
    ("ug", 1e-6),
    ("mcg", 1e-6),
    ("mg", 1e-3),
    ("g", 1.0),
];
/// Volume units with their size in litres.
const VOLUME: [(&str, f64); 4] = [("mL", 1e-3), ("ml", 1e-3), ("L", 1.0), ("l", 1.0)];
/// Time units.
const TIME: [&str; 4] = ["s", "min", "h", "d"];

fn mass(unit: &str) -> Option<f64> {
    MASS.iter().find(|(u, _)| *u == unit).map(|(_, g)| *g)
}

fn volume(unit: &str) -> Option<f64> {
    VOLUME.iter().find(|(u, _)| *u == unit).map(|(_, l)| *l)
}

fn invalid(which: &str, value: &str, reason: &str) -> NcaError {
    NcaError::InvalidUnits {
        which: which.to_string(),
        value: value.to_string(),
        reason: reason.to_string(),
    }
}

/// Checked units: the strings to print and the factor that turns dose/concentration·volume
/// quantities (CL, V) into litres.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Checked {
    time: String,
    conc: String,
    dose: String,
    /// (dose mass in the concentration's mass unit) × (concentration's volume unit in litres).
    pub volume_factor: f64,
}

/// NCA-UNIT-02: the three units must be known and consistent (a dose mass, a mass/volume
/// concentration, a time); otherwise a readable error before any computation.
pub(crate) fn check(units: &Units) -> Result<Checked, NcaError> {
    let time = units.time.trim();
    if !TIME.contains(&time) {
        return Err(invalid("time", &units.time, "use one of s, min, h, d"));
    }
    let dose = units.dose.trim();
    let Some(dose_grams) = mass(dose) else {
        let reason = if volume(dose).is_some() || dose.contains('/') {
            "the dose must be a mass (ng, µg, mg, g)"
        } else {
            "unknown unit; use ng, µg, mg or g"
        };
        return Err(invalid("dose", &units.dose, reason));
    };
    let conc = units.concentration.trim();
    let Some((conc_mass, conc_volume)) = conc.split_once('/') else {
        return Err(invalid(
            "concentration",
            &units.concentration,
            "a concentration is a mass per volume, e.g. ng/mL or mg/L",
        ));
    };
    let (Some(conc_grams), Some(litres)) = (mass(conc_mass.trim()), volume(conc_volume.trim()))
    else {
        return Err(invalid(
            "concentration",
            &units.concentration,
            "use a mass (ng, µg, mg, g) per volume (mL, L), e.g. ng/mL",
        ));
    };
    Ok(Checked {
        time: time.to_string(),
        conc: conc.to_string(),
        dose: dose.to_string(),
        volume_factor: dose_grams / conc_grams * litres,
    })
}

/// Parameters in dose / (concentration·time) or dose / concentration: converted to L/time or L.
pub(crate) const VOLUME_PARAMETERS: [&str; 9] = [
    "cl.obs",
    "cl.pred",
    "vz.obs",
    "vz.pred",
    "vss.obs",
    "vss.pred",
    "vss.iv.obs",
    "vss.iv.pred",
    "vss.iv.last",
];

impl Checked {
    /// NCA-UNIT-01: the unit of the parameter called `name` (PKNCA spelling); `""` for a
    /// dimensionless one, `None` for an unknown name.
    pub fn unit_of(&self, name: &str) -> Option<String> {
        let (t, c) = (&self.time, &self.conc);
        let auc = format!("{t}·{c}");
        let aumc = format!("{t}²·{c}");
        if let Some(base) = name.strip_suffix(".dn") {
            let unit = self.unit_of(base)?;
            return Some(format!("({unit})/{}", self.dose));
        }
        let unit = match name {
            "c0" | "cmax" | "clast.obs" | "clast.pred" => c.clone(),
            "tmax"
            | "tfirst"
            | "tlast"
            | "tlag"
            | "lambda.z.time.first"
            | "lambda.z.time.last"
            | "half.life"
            | "mrt.last"
            | "mrt.obs"
            | "mrt.pred"
            | "mrt.iv.last"
            | "mrt.iv.obs"
            | "mrt.iv.pred" => t.clone(),
            "lambda.z" => format!("1/{t}"),
            "auclast" | "aucall" | "aucinf.obs" | "aucinf.pred" | "aucivlast" | "aucivall"
            | "aucivinf.obs" | "aucivinf.pred" => auc,
            "aumclast" | "aumcall" | "aumcinf.obs" | "aumcinf.pred" => aumc,
            "cl.obs" | "cl.pred" => format!("L/{t}"),
            "vz.obs" | "vz.pred" | "vss.obs" | "vss.pred" | "vss.iv.obs" | "vss.iv.pred"
            | "vss.iv.last" => "L".to_string(),
            "aucpext.obs" | "aucpext.pred" | "aumcpext.obs" | "aumcpext.pred"
            | "aucivpbextlast" | "aucivpbextall" | "aucivpbextinf.obs" | "aucivpbextinf.pred" => {
                "%".to_string()
            }
            "r.squared" | "adj.r.squared" | "lambda.z.n.points" | "span.ratio" => String::new(),
            _ => return None,
        };
        Some(unit)
    }
}
