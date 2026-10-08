//! Display formatting: numbers for the eye, names for parameters. Never rounds a stored value.

/// A number with four significant digits (more for large integers in range), scientific notation
/// outside 1e-3 to 1e5. `-` for a value that is not finite.
pub fn number(x: f64) -> String {
    if !x.is_finite() {
        return "-".to_owned();
    }
    if x == 0.0 {
        return "0".to_owned();
    }
    let magnitude = x.abs().log10().floor();
    if !(-3.0..5.0).contains(&magnitude) {
        return format!("{x:.3e}");
    }
    let decimals = (3.0 - magnitude).clamp(0.0, 6.0) as usize;
    let text = format!("{x:.decimals$}");
    // Drop trailing zeros after a decimal point, but keep the digits that carry information.
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// A goodness of fit: up to five decimals, enough to tell 0.9999 from 1.
pub fn fit_quality(x: f64) -> String {
    if !x.is_finite() {
        return "-".to_owned();
    }
    let text = format!("{x:.5}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A number the way it was typed: the shortest text that reads back as the same value.
pub fn exact(x: f64) -> String {
    format!("{x}")
}

/// The label of a PKNCA parameter name in the summary: `lambda.z` is `λz`, `aucinf.obs` is
/// `AUCinf`. Names the table does not know are shown as they are.
pub fn parameter_label(name: &str, extravascular: bool) -> String {
    let f = if extravascular { "/F" } else { "" };
    match name {
        "cmax" => "Cmax".to_owned(),
        "tmax" => "Tmax".to_owned(),
        "tlast" => "Tlast".to_owned(),
        "clast.obs" => "Clast".to_owned(),
        "auclast" => "AUClast".to_owned(),
        "aucall" => "AUCall".to_owned(),
        "aucinf.obs" => "AUCinf".to_owned(),
        "aucinf.pred" => "AUCinf (predicted Clast)".to_owned(),
        "aucpext.obs" => "AUC extrapolated".to_owned(),
        "aucpext.pred" => "AUC extrapolated (predicted Clast)".to_owned(),
        "lambda.z" => "λz".to_owned(),
        "half.life" => "t½".to_owned(),
        "r.squared" => "R²".to_owned(),
        "adj.r.squared" => "Adjusted R²".to_owned(),
        "span.ratio" => "Span ratio".to_owned(),
        "lambda.z.n.points" => "Points in λz".to_owned(),
        "mrt.obs" => "MRT".to_owned(),
        "mrt.iv.obs" => "MRT (IV)".to_owned(),
        "cl.obs" => format!("CL{f}"),
        "vz.obs" => format!("Vz{f}"),
        "vss.iv.obs" => "Vss".to_owned(),
        "c0" => "C0".to_owned(),
        other => other.to_owned(),
    }
}

/// A sentence of the engine with the parameter names it quotes written as the results table
/// labels them (`aucpext.obs` is `AUC extrapolated`).
pub fn with_labels(sentence: &str) -> String {
    // Longer names first: `lambda.z.n.points` before `lambda.z`.
    const NAMES: [&str; 9] = [
        "lambda.z.n.points",
        "aucpext.pred",
        "aucpext.obs",
        "adj.r.squared",
        "span.ratio",
        "aucinf.pred",
        "aucinf.obs",
        "lambda.z",
        "auclast",
    ];
    let mut out = sentence.to_owned();
    for name in NAMES {
        out = out.replace(name, &parameter_label(name, true));
    }
    out
}

/// The unit of a parameter from the units of the worksheet columns and the derived units the
/// engine names; `None` when it has none or the units are not known.
pub fn parameter_unit(
    name: &str,
    time: Option<&str>,
    conc: Option<&str>,
    derived: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    let derived = |key: &str| derived.get(key).cloned();
    match name {
        "cmax" | "clast.obs" | "clast.pred" | "c0" => conc.map(str::to_owned),
        "tmax"
        | "tlast"
        | "tfirst"
        | "tlag"
        | "half.life"
        | "mrt.obs"
        | "mrt.pred"
        | "mrt.last"
        | "mrt.iv.obs"
        | "mrt.iv.pred"
        | "mrt.iv.last"
        | "lambda.z.time.first"
        | "lambda.z.time.last" => time.map(str::to_owned),
        "auclast" | "aucall" | "aucinf.obs" | "aucinf.pred" => derived("auc"),
        "aumclast" | "aumcall" | "aumcinf.obs" | "aumcinf.pred" => derived("aumc"),
        "lambda.z" => derived("lambda_z"),
        "cl.obs" | "cl.pred" => derived("cl"),
        "vz.obs" | "vz.pred" | "vss.iv.obs" | "vss.iv.pred" | "vss.obs" | "vss.pred"
        | "vss.iv.last" => derived("v"),
        "aucpext.obs" | "aucpext.pred" | "aumcpext.obs" | "aumcpext.pred" => Some("%".to_owned()),
        _ => None,
    }
}

/// `snake_case` as words: `no_valid_fit` is `no valid fit`.
pub fn words(code: &str) -> String {
    code.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn numbers_show_four_significant_digits() {
        for (x, text) in [
            (0.0, "0"),
            (3.971, "3.971"),
            (12.3456, "12.35"),
            (123.456, "123.5"),
            (1234.56, "1235"),
            (99999.0, "99999"),
            (100000.0, "1.000e5"),
            (0.0123456, "0.01235"),
            (0.000123456, "1.235e-4"),
            (-2.5, "-2.5"),
            (2.0, "2"),
            (0.5, "0.5"),
        ] {
            assert_eq!(number(x), text, "{x}");
        }
        assert_eq!(number(f64::NAN), "-");
        assert_eq!(number(f64::INFINITY), "-");
    }

    #[test]
    fn flag_sentences_use_the_labels_of_the_results_table() {
        assert_eq!(
            with_labels(
                "aucpext.obs is 37.3 % (> 20 %); the AUC to infinity relies mostly on extrapolation"
            ),
            "AUC extrapolated is 37.3 % (> 20 %); the AUC to infinity relies mostly on extrapolation"
        );
        assert!(
            with_labels("aucpext.pred is 5 %").starts_with("AUC extrapolated (predicted Clast) is")
        );
        assert_eq!(with_labels("nothing to change"), "nothing to change");
    }

    #[test]
    fn a_fit_quality_keeps_the_digits_that_matter() {
        assert_eq!(fit_quality(0.99994), "0.99994");
        assert_eq!(fit_quality(0.999999), "1");
        assert_eq!(fit_quality(0.9), "0.9");
        assert_eq!(fit_quality(f64::NAN), "-");
    }

    #[test]
    fn the_exact_text_reads_back() {
        for x in [0.1 + 0.2, 1.0 / 3.0, 1e-300, 123456789.123456] {
            assert_eq!(exact(x).parse::<f64>().unwrap(), x);
        }
    }

    #[test]
    fn labels_and_units_follow_the_route_and_the_columns() {
        assert_eq!(parameter_label("cl.obs", true), "CL/F");
        assert_eq!(parameter_label("cl.obs", false), "CL");
        assert_eq!(parameter_label("lambda.z", true), "λz");
        assert_eq!(parameter_label("something.else", true), "something.else");
        let derived: BTreeMap<String, String> = [
            ("auc".to_owned(), "h*mg/L".to_owned()),
            ("cl".to_owned(), "L/h".to_owned()),
        ]
        .into();
        assert_eq!(
            parameter_unit("auclast", Some("h"), Some("mg/L"), &derived).as_deref(),
            Some("h*mg/L")
        );
        assert_eq!(
            parameter_unit("cmax", Some("h"), Some("mg/L"), &derived).as_deref(),
            Some("mg/L")
        );
        assert_eq!(
            parameter_unit("half.life", Some("h"), None, &derived).as_deref(),
            Some("h")
        );
        assert_eq!(
            parameter_unit("aucpext.obs", None, None, &derived).as_deref(),
            Some("%")
        );
        assert_eq!(parameter_unit("vz.obs", Some("h"), None, &derived), None);
        assert_eq!(parameter_unit("r.squared", None, None, &derived), None);
        assert_eq!(words("no_valid_fit"), "no valid fit");
    }
}
