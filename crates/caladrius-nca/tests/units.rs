#![allow(clippy::unwrap_used, clippy::panic)] // test code: helpers panic on purpose
//! Units of the NCA (`specs/nca.md` NCA-UNIT-01, UNIT-02; task T-025): explicit units are data,
//! every result carries its unit, CL and volumes come out in litres, unknown or inconsistent units
//! are refused before any computation, and an analysis without units is unchanged and flagged.

use caladrius_nca::{NcaError, NcaInput, NcaOptions, NcaResult, Route, Units, run};

/// Theophylline-like oral profile: 10 mg, concentrations in ng/mL, times in h.
const T: [f64; 11] = [
    0.0, 0.25, 0.57, 1.12, 2.02, 3.82, 5.1, 7.03, 9.05, 12.12, 24.37,
];
const C: [f64; 11] = [
    0.0, 284.0, 657.0, 1050.0, 966.0, 858.0, 836.0, 747.0, 689.0, 594.0, 328.0,
];

fn units(time: &str, concentration: &str, dose: &str) -> Units {
    Units {
        time: time.to_string(),
        concentration: concentration.to_string(),
        dose: dose.to_string(),
    }
}

fn nca(units: Option<Units>) -> Result<NcaResult, NcaError> {
    run(&NcaInput {
        time: T.to_vec(),
        conc: C.to_vec(),
        dose: 10.0,
        route: Route::Extravascular,
        options: NcaOptions {
            units,
            ..NcaOptions::default()
        },
    })
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * b.abs()
}

#[test]
fn ten_mg_oral_in_ng_per_ml_gives_cl_in_litres_per_hour() {
    let r = nca(Some(units("h", "ng/mL", "mg"))).unwrap();
    assert!(!r.units_missing());
    // CL/F = dose / AUCinf with the dose in ng (1e7) and AUC in h·ng/mL: mL/h, then L/h.
    let auc = r.get("aucinf.obs").unwrap();
    assert!(close(r.get("cl.obs").unwrap(), 10.0e6 / auc / 1000.0));
    assert_eq!(r.unit("cl.obs"), Some("L/h"));
    assert!(close(
        r.get("vz.obs").unwrap(),
        r.get("cl.obs").unwrap() / r.get("lambda.z").unwrap()
    ));
    for (name, unit) in [
        ("vz.obs", "L"),
        ("cmax", "ng/mL"),
        ("tmax", "h"),
        ("lambda.z", "1/h"),
        ("half.life", "h"),
        ("auclast", "h·ng/mL"),
        ("aumclast", "h²·ng/mL"),
        ("mrt.obs", "h"),
        ("aucpext.obs", "%"),
        ("r.squared", ""),
        ("cmax.dn", "(ng/mL)/mg"),
    ] {
        assert_eq!(r.unit(name), Some(unit), "{name}");
    }
}

#[test]
fn units_change_only_cl_and_volumes() {
    let plain = nca(None).unwrap();
    let with = nca(Some(units("h", "ng/mL", "mg"))).unwrap();
    assert!(plain.units_missing());
    assert_eq!(plain.unit("cl.obs"), None);
    for p in plain.parameters() {
        let (a, b) = (p.value.value(), with.get(&p.name));
        let scaled = matches!(
            p.name.as_str(),
            "cl.obs" | "cl.pred" | "vz.obs" | "vz.pred" | "vss.obs" | "vss.pred"
        );
        match (a, b) {
            (Some(a), Some(b)) if scaled => assert!(close(b, a * 1000.0), "{}", p.name),
            (a, b) => assert_eq!(a, b, "{}", p.name),
        }
    }
    // Same mass in dose and concentration and litres: nothing to convert.
    let same = nca(Some(units("h", "mg/L", "mg"))).unwrap();
    assert_eq!(same.get("cl.obs"), plain.get("cl.obs"));
}

#[test]
fn unknown_or_inconsistent_units_are_refused() {
    for (u, which, hint) in [
        (units("hours", "ng/mL", "mg"), "time", "s, min, h, d"),
        (units("h", "ng/mL", "mL"), "dose", "must be a mass"),
        (units("h", "ng", "mg"), "concentration", "mass per volume"),
        (units("h", "ng/kg", "mg"), "concentration", "per volume"),
        (units("h", "ng/mL", "tablet"), "dose", "unknown unit"),
    ] {
        match nca(Some(u)) {
            Err(NcaError::InvalidUnits {
                which: w, reason, ..
            }) => {
                assert_eq!(w, which);
                assert!(reason.contains(hint), "{reason}");
            }
            other => panic!("{which}: {other:?}"),
        }
    }
    // Spelling variants of the table.
    assert!(nca(Some(units("min", "µg/L", "ug"))).is_ok());
    assert!(nca(Some(units("d", "mcg/ml", "g"))).is_ok());
}

#[test]
fn units_are_data() {
    let options = NcaOptions {
        units: Some(units("h", "ng/mL", "mg")),
        ..NcaOptions::default()
    };
    let text = serde_json::to_string(&options).unwrap();
    assert_eq!(serde_json::from_str::<NcaOptions>(&text).unwrap(), options);
    assert!(
        serde_json::from_str::<NcaOptions>(r#"{"units":{"time":"h","conc":"ng/mL","dose":"mg"}}"#)
            .is_err()
    );
    let r = nca(Some(units("h", "ng/mL", "mg"))).unwrap();
    let text = serde_json::to_string(&r).unwrap();
    assert!(text.contains(r#""unit":"L/h""#), "{text}");
    assert_eq!(serde_json::from_str::<NcaResult>(&text).unwrap(), r);
    // Without units nothing changes in the serialized parameters.
    let plain = serde_json::to_string(&nca(None).unwrap()).unwrap();
    assert!(!plain.contains(r#""unit""#));
}
