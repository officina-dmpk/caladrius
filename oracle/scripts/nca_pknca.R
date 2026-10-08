#!/usr/bin/env Rscript
# Caladrius public NCA oracle (task T-003).
#
# Produces, with the R package PKNCA, the expected NCA results for the public
# datasets Theoph (oral) and Indometh (IV bolus) shipped with R.
#
# Usage, from the repository root:
#   Rscript oracle/scripts/nca_pknca.R
#
# Writes (and overwrites, deterministically; no timestamps):
#   oracle/data/theoph.csv, oracle/data/indometh.csv       input profiles
#   oracle/expected/<case>.csv                             long table: subject,parameter,value
#   oracle/expected/<case>.options.json                    options, versions, preprocessing
#
# Cases: theoph, theoph_linear, indometh, indometh_linear. The first of each pair
# uses the "lin up/log down" AUC rule, the second the pure linear trapezoidal rule.
# Synthetic cases (task T-005): synthetic_lz and synthetic_lz_f1e3 on the hand-made
# profiles of oracle/data/synthetic_lz.csv (read, never written, by this script); they
# settle the lambda_z open items O-01 and O-02 of specs/nca.md. A case may override
# PKNCA options with the `options` field of its entry in `cases`.
# Edge-case cases (task T-012), from the hand-made profiles oracle/data/edge_*.csv: BLQ policies,
# missing and negative values, IV infusion, Tlag, dose-normalised values, percent
# back-extrapolated, MRT to Tlast, plain Vss, percent AUMC extrapolated. They are run on the
# profile as given (no added C0 point); see oracle/data/README.md.
# Every option that can change a number is set explicitly below; none relies on a
# PKNCA default. Never edit the generated files by hand.

suppressPackageStartupMessages({
  library(PKNCA)
  library(jsonlite)
})

# ---------------------------------------------------------------- locations
args_all <- commandArgs(trailingOnly = FALSE)
file_arg <- sub("^--file=", "", args_all[grepl("^--file=", args_all)])
root <- if (length(file_arg) == 1L) {
  normalizePath(file.path(dirname(file_arg), "..", ".."), winslash = "/", mustWork = TRUE)
} else {
  normalizePath(".", winslash = "/", mustWork = TRUE)
}
data_dir <- file.path(root, "oracle", "data")
exp_dir <- file.path(root, "oracle", "expected")
dir.create(data_dir, showWarnings = FALSE, recursive = TRUE)
dir.create(exp_dir, showWarnings = FALSE, recursive = TRUE)

# Text files are written in binary mode so that line endings are always LF.
write_lines_lf <- function(lines, path) {
  con <- file(path, open = "wb")
  on.exit(close(con))
  writeLines(lines, con, sep = "\n", useBytes = TRUE)
}
num <- function(x) ifelse(is.na(x), "", sprintf("%.15g", x))

# ---------------------------------------------------------------- explicit options
base_options <- list(
  conc.na = "drop",
  conc.blq = list(first = "keep", middle = "drop", last = "keep"),
  first.tmax = TRUE,
  allow.tmax.in.half.life = FALSE,
  min.hl.points = 3,
  adj.r.squared.factor = 1e-4,
  min.hl.r.squared = 0.9,
  min.span.ratio = 2,
  max.aucinf.pext = 20,
  max.missing = 0.5
)

# ---------------------------------------------------------------- datasets
# Theoph: oral. The absolute dose in mg is Dose (mg/kg) * Wt (kg).
th <- data.frame(
  subject = as.integer(as.character(Theoph$Subject)),
  time = Theoph$Time,
  conc = Theoph$conc,
  dose = Theoph$Dose * Theoph$Wt
)
# Indometh: IV bolus. The R documentation gives no dose; 25 mg is a test constant
# (see oracle/data/README.md). Results that scale with dose (CL, Vz, Vss) are
# exact for that constant.
indometh_dose_mg <- 25
im <- data.frame(
  subject = as.integer(as.character(Indometh$Subject)),
  time = Indometh$time,
  conc = Indometh$conc,
  dose = indometh_dose_mg
)
sort_profile <- function(d) d[order(d$subject, d$time), ]
th <- sort_profile(th)
im <- sort_profile(im)

write_profile <- function(d, path) {
  write_lines_lf(c("subject,time,conc,dose",
                   sprintf("%d,%s,%s,%s", d$subject, num(d$time), num(d$conc), num(d$dose))), path)
}
write_profile(th, file.path(data_dir, "theoph.csv"))
write_profile(im, file.path(data_dir, "indometh.csv"))

# From here on the inputs are read back from the CSV, so that R and the Rust
# tests see exactly the same decimal text.
read_profile <- function(path) {
  d <- read.csv(path, stringsAsFactors = FALSE)
  d[order(d$subject, d$time), ]
}
th <- read_profile(file.path(data_dir, "theoph.csv"))
im <- read_profile(file.path(data_dir, "indometh.csv"))
syn <- read_profile(file.path(data_dir, "synthetic_lz.csv"))
edge_blq <- read_profile(file.path(data_dir, "edge_blq.csv"))
edge_missing <- read_profile(file.path(data_dir, "edge_missing.csv"))
edge_negative <- read_profile(file.path(data_dir, "edge_negative.csv"))
edge_oral <- read_profile(file.path(data_dir, "edge_oral.csv"))
edge_iv <- read_profile(file.path(data_dir, "edge_iv.csv"))
edge_infusion <- read_profile(file.path(data_dir, "edge_infusion.csv"))

# ---------------------------------------------------------------- PKNCA run
param_oral <- c(
  "cmax", "tmax", "tfirst", "tlast", "clast.obs",
  "lambda.z", "r.squared", "adj.r.squared", "lambda.z.time.first",
  "lambda.z.time.last", "lambda.z.n.points", "clast.pred", "half.life", "span.ratio",
  "auclast", "aucall", "aumclast",
  "aucinf.obs", "aucinf.pred", "aumcinf.obs", "aumcinf.pred",
  "aucpext.obs", "aucpext.pred",
  "cl.obs", "cl.pred", "mrt.obs", "mrt.pred", "vz.obs", "vz.pred"
)
param_iv <- c(
  "c0", "cmax", "tmax", "tfirst", "tlast", "clast.obs",
  "lambda.z", "r.squared", "adj.r.squared", "lambda.z.time.first",
  "lambda.z.time.last", "lambda.z.n.points", "clast.pred", "half.life", "span.ratio",
  "auclast", "aucall", "aumclast",
  "aucinf.obs", "aucinf.pred", "aumcinf.obs", "aumcinf.pred",
  "aucpext.obs", "aucpext.pred",
  "cl.obs", "cl.pred", "mrt.iv.obs", "mrt.iv.pred", "vz.obs", "vz.pred",
  "vss.iv.obs", "vss.iv.pred"
)

# Options of a case: the explicit base, overridden by the case. A `conc.blq` override replaces the
# whole rule (merging would leave the first/middle/last classes next to before/after Tmax).
merge_options <- function(option_overrides) {
  opts <- modifyList(base_options, option_overrides)
  if ("conc.blq" %in% names(option_overrides)) opts$conc.blq <- option_overrides$conc.blq
  opts
}

run_pknca <- function(profile, route, auc_method, params, option_overrides = list(), duration = 0) {
  if ("excl_hl" %in% names(profile)) {
    conc <- PKNCAconc(profile[, c("subject", "time", "conc", "excl_hl")], conc ~ time | subject,
                      exclude_half.life = "excl_hl")
  } else {
    conc <- PKNCAconc(profile[, c("subject", "time", "conc")], conc ~ time | subject)
  }
  dose_rows <- profile[!duplicated(profile$subject), c("subject", "dose")]
  dose_rows$time <- 0
  if (duration > 0) {
    dose_rows$duration <- duration
    dose <- PKNCAdose(dose_rows, dose ~ time | subject, route = route, duration = "duration")
  } else {
    dose <- PKNCAdose(dose_rows, dose ~ time | subject, route = route)
  }
  iv <- data.frame(start = 0, end = Inf)
  for (p in params) iv[[p]] <- TRUE
  opts <- merge_options(option_overrides)
  opts$auc.method <- auc_method
  d <- PKNCAdata(conc, dose, intervals = iv, options = opts)
  res <- suppressWarnings(as.data.frame(pk.nca(d)))
  res <- res[res$PPTESTCD %in% params, c("subject", "PPTESTCD", "PPORRES")]
  names(res) <- c("subject", "parameter", "value")
  res$subject <- as.integer(as.character(res$subject))
  res
}

# IV bolus without a sample at the dose time: C0 is back-extrapolated on the
# log scale from the first two positive concentrations (PKNCA pk.calc.c0,
# method "logslope") and added as a profile point at t = 0 for the AUC, AUMC and
# derived parameters. Cmax, Tmax and Tfirst stay those of the observed samples.
augment_with_c0 <- function(profile) {
  out <- list()
  for (s in sort(unique(profile$subject))) {
    p <- profile[profile$subject == s, ]
    if (!any(p$time == 0)) {
      c0 <- pk.calc.c0(p$conc, p$time, time.dose = 0, method = "logslope")
      # Terminal-phase selection must not see the added point, and must still
      # exclude the observed Cmax (it would otherwise be eligible, because the
      # added point becomes the profile maximum).
      p$excl_hl <- p$time == p$time[which.max(p$conc)]
      p <- rbind(data.frame(subject = s, time = 0, conc = c0, dose = p$dose[1], excl_hl = TRUE), p)
    }
    if (!("excl_hl" %in% names(p))) p$excl_hl <- FALSE
    out[[length(out) + 1L]] <- p
  }
  do.call(rbind, out)
}

build_case <- function(profile, route, auc_method, params, option_overrides = list()) {
  if (route == "extravascular") {
    return(run_pknca(profile, route, auc_method, params, option_overrides))
  }
  observed <- run_pknca(profile, route, auc_method, params, option_overrides)
  augmented <- run_pknca(augment_with_c0(profile), route, auc_method, params, option_overrides)
  # The back-extrapolated point must not change the terminal-phase selection.
  lz_obs <- observed[observed$parameter == "lambda.z", ]
  lz_aug <- augmented[augmented$parameter == "lambda.z", ]
  stopifnot(isTRUE(all.equal(lz_obs$value, lz_aug$value, tolerance = 1e-12)))
  keep_observed <- c("cmax", "tmax", "tfirst")
  rbind(
    observed[observed$parameter %in% keep_observed, ],
    augmented[!(augmented$parameter %in% keep_observed), ]
  )
}

# ---------------------------------------------------------------- edge-case parameter lists
# `aumcpext.*` are not PKNCA parameters: the script derives them from PKNCA's aumclast and
# aumcinf (NCA-EXT-03b: 100 * (1 - AUMClast / AUMCinf)), and says so in the options file.
param_dn <- c("cmax.dn", "clast.obs.dn", "auclast.dn", "aucall.dn", "aucinf.obs.dn",
              "aucinf.pred.dn", "aumclast.dn", "aumcall.dn", "aumcinf.obs.dn", "aumcinf.pred.dn")
derived_aumcpext <- c("aumcpext.obs", "aumcpext.pred")
param_edge_core <- c(
  "cmax", "tmax", "tfirst", "tlast", "clast.obs",
  "lambda.z", "r.squared", "adj.r.squared", "lambda.z.time.first",
  "lambda.z.time.last", "lambda.z.n.points", "clast.pred", "half.life", "span.ratio",
  "auclast", "aucall", "aumclast", "aumcall",
  "aucinf.obs", "aucinf.pred", "aumcinf.obs", "aumcinf.pred",
  "aucpext.obs", "aucpext.pred", derived_aumcpext
)
param_edge_blq <- c(param_edge_core, "tlag", "cl.obs", "cl.pred", "mrt.obs", "mrt.pred", "mrt.last",
                    "vz.obs", "vz.pred")
param_edge_oral <- c(param_edge_core, "tlag", "cl.obs", "cl.pred", "mrt.obs", "mrt.pred", "mrt.last",
                     "vz.obs", "vz.pred", "vss.obs", "vss.pred", param_dn)
param_edge_iv <- c(
  "c0", param_edge_core,
  "cl.obs", "cl.pred", "mrt.iv.obs", "mrt.iv.pred", "mrt.iv.last", "vz.obs", "vz.pred",
  "vss.iv.obs", "vss.iv.pred", "vss.iv.last", "vss.obs", "vss.pred",
  "aucivlast", "aucivall", "aucivinf.obs", "aucivinf.pred",
  "aucivpbextlast", "aucivpbextall", "aucivpbextinf.obs", "aucivpbextinf.pred", param_dn
)
param_edge_infusion <- c(
  param_edge_core,
  "cl.obs", "cl.pred", "mrt.iv.obs", "mrt.iv.pred", "mrt.iv.last", "vz.obs", "vz.pred",
  "vss.iv.obs", "vss.iv.pred", "vss.iv.last", "vss.obs", "vss.pred", param_dn
)
param_edge_negative <- c(param_edge_core, "cl.obs", "mrt.obs", "vz.obs")

edge_note <- "100 mg oral (test constant)"
blq_case <- function(name, rule, note) {
  list(name = name, dataset = "edge_blq", profile = edge_blq, route = "extravascular",
       auc_method = "lin up/log down", params = param_edge_blq, dose_note = edge_note,
       options = list(conc.blq = rule), direct = TRUE,
       engine = list(start = "none"), case_note = note)
}
missing_case <- function(name, rule, note) {
  list(name = name, dataset = "edge_missing", profile = edge_missing, route = "extravascular",
       auc_method = "lin up/log down", params = param_edge_core, dose_note = edge_note,
       options = list(conc.na = rule), direct = TRUE,
       engine = list(start = "none"), case_note = note)
}

edge_cases <- list(
  blq_case("edge_blq_default", list(first = "keep", middle = "drop", last = "keep"),
           "PKNCA default policy written out: zeros before the first positive kept, interior zeros dropped, trailing zeros kept"),
  blq_case("edge_blq_keep", list(first = "keep", middle = "keep", last = "keep"),
           "every zero kept"),
  blq_case("edge_blq_last_drop", list(first = "keep", middle = "keep", last = "drop"),
           "interior zeros kept, trailing zeros dropped"),
  blq_case("edge_blq_first_drop", list(first = "drop", middle = "drop", last = "drop"),
           "every zero dropped, including the one at the dose time: no AUC without a start concentration"),
  blq_case("edge_blq_set", list(first = "keep", middle = 0.3, last = 0.05),
           "interior zeros replaced by 0.3, trailing zeros by 0.05, leading zeros kept"),
  blq_case("edge_blq_tmax", list(before.tmax = "keep", after.tmax = "drop"),
           "classes by position relative to Tmax: zeros before Tmax kept, zeros from Tmax on dropped"),
  missing_case("edge_missing_drop", "drop", "missing concentrations dropped with their times"),
  missing_case("edge_missing_replace", 1,
               "missing concentrations replaced by 1 before any other step"),
  list(name = "edge_negative_linear", dataset = "edge_negative", profile = edge_negative,
       route = "extravascular", auc_method = "linear", params = param_edge_negative,
       dose_note = edge_note, options = list(), direct = TRUE,
       engine = list(start = "none", negative = "allow"),
       case_note = "negative concentrations kept (PKNCA only warns); linear rule, because lin up/log down gives NaN areas on a negative value",
       # D-01 (specs/differences.md): PKNCA takes the trailing negative value of subject 2 as Tlast
       # and Clast, Caladrius never does (NCA-DAT-04). The values below follow from that choice.
       documented_differences = list(list(
         id = "D-01", subjects = I("2"),
         parameters = I(c("tlast", "clast.obs", "clast.pred", "auclast", "aumclast",
                          "aucinf.obs", "aucinf.pred", "aumcinf.obs", "aumcinf.pred",
                          "aucpext.obs", "aucpext.pred", "aumcpext.obs", "aumcpext.pred",
                          "cl.obs", "mrt.obs", "vz.obs")),
         note = "PKNCA takes the trailing negative concentration as Tlast and Clast; Caladrius never does"))),
  list(name = "edge_oral", dataset = "edge_oral", profile = edge_oral, route = "extravascular",
       auc_method = "lin up/log down", params = param_edge_oral,
       dose_note = "dose per subject, 20 to 250 mg (test constants)", options = list(),
       direct = TRUE, engine = list(start = "none"),
       case_note = "Tlag, MRT to Tlast, plain Vss, dose-normalised values and percent AUMC extrapolated on oral profiles with and without a lag"),
  list(name = "edge_iv", dataset = "edge_iv", profile = edge_iv, route = "intravascular",
       auc_method = "lin up/log down", params = param_edge_iv,
       dose_note = "25 mg IV bolus (test constant)", options = list(), direct = TRUE,
       engine = list(start = "none"),
       case_note = "IV bolus with a record at the dose time (observed C0, or a zero placeholder): C0 methods, percent back-extrapolated, IV AUC"),
  list(name = "edge_iv_linear", dataset = "edge_iv", profile = edge_iv, route = "intravascular",
       auc_method = "linear", params = param_edge_iv,
       dose_note = "25 mg IV bolus (test constant)", options = list(), direct = TRUE,
       engine = list(start = "none"),
       case_note = "the same profiles with the linear rule"),
  list(name = "edge_infusion", dataset = "edge_infusion", profile = edge_infusion,
       route = "intravascular", duration = 2, auc_method = "lin up/log down",
       params = param_edge_infusion, dose_note = "dose per subject, 50 to 200 mg (test constants), infused over 2 h",
       options = list(), direct = TRUE, engine = list(start = "none"),
       case_note = "IV infusion of 2 h: MRT and Vss corrected for the infusion duration, lambda_z only after the end of the infusion"),
  list(name = "edge_infusion_linear", dataset = "edge_infusion", profile = edge_infusion,
       route = "intravascular", duration = 2, auc_method = "linear",
       params = param_edge_infusion, dose_note = "dose per subject, 50 to 200 mg (test constants), infused over 2 h",
       options = list(), direct = TRUE, engine = list(start = "none"),
       case_note = "the same infusion profiles with the linear rule")
)

cases <- list(
  list(name = "theoph", dataset = "theoph", profile = th, route = "extravascular",
       auc_method = "lin up/log down", params = param_oral,
       dose_note = "dose (mg) = Dose (mg/kg) * Wt (kg) per subject"),
  list(name = "theoph_linear", dataset = "theoph", profile = th, route = "extravascular",
       auc_method = "linear", params = param_oral,
       dose_note = "dose (mg) = Dose (mg/kg) * Wt (kg) per subject"),
  list(name = "indometh", dataset = "indometh", profile = im, route = "intravascular",
       auc_method = "lin up/log down", params = param_iv,
       dose_note = "25 mg IV bolus (test constant)"),
  list(name = "indometh_linear", dataset = "indometh", profile = im, route = "intravascular",
       auc_method = "linear", params = param_iv,
       dose_note = "25 mg IV bolus (test constant)"),
  list(name = "synthetic_lz", dataset = "synthetic_lz", profile = syn, route = "extravascular",
       auc_method = "lin up/log down", params = param_oral,
       dose_note = "100 mg oral (test constant)", options = list()),
  list(name = "synthetic_lz_f1e3", dataset = "synthetic_lz", profile = syn, route = "extravascular",
       auc_method = "lin up/log down", params = param_oral,
       dose_note = "100 mg oral (test constant)", options = list(adj.r.squared.factor = 1e-3))
)
cases <- c(cases, edge_cases)

versions <- list(
  R = R.version.string,
  PKNCA = as.character(packageVersion("PKNCA")),
  jsonlite = as.character(packageVersion("jsonlite"))
)

for (cs in cases) {
  overrides <- if (is.null(cs$options)) list() else cs$options
  duration <- if (is.null(cs$duration)) 0 else cs$duration
  subjects <- sort(unique(cs$profile$subject))
  if (isTRUE(cs$direct)) {
    pknca_params <- setdiff(cs$params, derived_aumcpext)
    res <- run_pknca(cs$profile, cs$route, cs$auc_method, pknca_params, overrides, duration)
    # A parameter PKNCA does not return for a subject (for example Tlag of a profile that never
    # rises) is NA; the grid is completed so that every subject has every parameter.
    grid <- expand.grid(subject = subjects, parameter = pknca_params, stringsAsFactors = FALSE)
    grid$value <- res$value[match(paste(grid$subject, grid$parameter), paste(res$subject, res$parameter))]
    res <- grid
    if (any(derived_aumcpext %in% cs$params)) {
      pick <- function(name) res$value[match(paste(subjects, name), paste(res$subject, res$parameter))]
      extra <- rbind(
        data.frame(subject = subjects, parameter = "aumcpext.obs",
                   value = 100 * (1 - pick("aumclast") / pick("aumcinf.obs"))),
        data.frame(subject = subjects, parameter = "aumcpext.pred",
                   value = 100 * (1 - pick("aumclast") / pick("aumcinf.pred"))))
      res <- rbind(res, extra)
    }
  } else {
    res <- build_case(cs$profile, cs$route, cs$auc_method, cs$params, overrides)
  }
  res$parameter <- factor(res$parameter, levels = cs$params)
  res <- res[order(res$subject, res$parameter), ]
  res$parameter <- as.character(res$parameter)
  stopifnot(nrow(res) == length(subjects) * length(cs$params))

  write_lines_lf(c("subject,parameter,value",
                   sprintf("%d,%s,%s", res$subject, res$parameter, num(res$value))),
                 file.path(exp_dir, paste0(cs$name, ".csv")))

  opts <- merge_options(overrides)
  opts$auc.method <- cs$auc_method
  meta <- list(
    schema = 1L,
    case = cs$name,
    dataset = cs$dataset,
    data_file = paste0("data/", cs$dataset, ".csv"),
    generated_by = "oracle/scripts/nca_pknca.R",
    route = if (cs$route == "extravascular") "extravascular" else if (duration > 0) "iv_infusion" else "iv_bolus",
    units = list(dose = "mg", time = "h", concentration = "mg/L"),
    dose = cs$dose_note,
    versions = versions,
    pknca_options = opts,
    pknca_dose_route = cs$route,
    interval = list(start = 0, end = "Inf"),
    parameter_notes = list(
      aucpext = "percentage of AUC(0-inf) that is extrapolated, in percent",
      lambda.z = "1/h; selected by PKNCA best adjusted R squared among the last 3 or more points, Tmax excluded",
      half.life = "ln(2) / lambda.z, in h",
      mrt = "extravascular: mrt.obs/mrt.pred = AUMC/AUC (no absorption-time correction); IV: mrt.iv.obs/mrt.iv.pred",
      cl = "dose / AUC(0-inf); this is CL/F for the oral case",
      vz = "dose / (lambda.z * AUC(0-inf)); this is Vz/F for the oral case"
    ),
    preprocessing = if (isTRUE(cs$direct)) {
      list("none: the profile is given to PKNCA as it is in the data file; no point is added")
    } else if (cs$route == "extravascular") {
      list("none: every subject has a sample at the dose time")
    } else {
      list(
        "C0 is back-extrapolated on the log scale from the first two positive concentrations (pk.calc.c0, method logslope) and added as a point at t = 0 for AUC, AUMC and derived parameters",
        "cmax, tmax and tfirst are taken from the observed samples only, without the back-extrapolated point",
        "the added point and the observed Cmax point are excluded from the terminal-phase selection (PKNCA exclude_half.life), so that lambda.z is the one obtained on the observed samples only; the script asserts that lambda.z is identical with and without the added point"
      )
    },
    n_subjects = length(subjects),
    parameters = cs$params,
    n_values = nrow(res),
    n_na = sum(is.na(res$value))
  )
  if (isTRUE(cs$direct)) {
    # Fields of the edge-case cases only (the older options files do not have them).
    meta$case_note <- cs$case_note
    if (duration > 0) meta$infusion_duration <- duration
    meta$engine <- cs$engine
    if (!is.null(cs$documented_differences)) meta$documented_differences <- cs$documented_differences
    if (any(derived_aumcpext %in% cs$params)) {
      meta$derived_parameters <- list(
        "aumcpext.obs" = "100 * (1 - aumclast / aumcinf.obs), computed by the script: PKNCA has no such parameter",
        "aumcpext.pred" = "100 * (1 - aumclast / aumcinf.pred), computed by the script: PKNCA has no such parameter")
    }
  }
  write_lines_lf(toJSON(meta, pretty = TRUE, auto_unbox = TRUE, digits = NA, null = "null"),
                 file.path(exp_dir, paste0(cs$name, ".options.json")))
  cat(sprintf("%-16s %3d subjects, %4d values (%d NA)\n", cs$name, length(subjects), nrow(res), sum(is.na(res$value))))
}
cat(sprintf("%s | PKNCA %s | jsonlite %s\n", versions$R, versions$PKNCA, versions$jsonlite))
