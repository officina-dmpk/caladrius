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

run_pknca <- function(profile, route, auc_method, params, option_overrides = list()) {
  if ("excl_hl" %in% names(profile)) {
    conc <- PKNCAconc(profile[, c("subject", "time", "conc", "excl_hl")], conc ~ time | subject,
                      exclude_half.life = "excl_hl")
  } else {
    conc <- PKNCAconc(profile[, c("subject", "time", "conc")], conc ~ time | subject)
  }
  dose_rows <- profile[!duplicated(profile$subject), c("subject", "dose")]
  dose_rows$time <- 0
  dose <- PKNCAdose(dose_rows, dose ~ time | subject, route = route)
  iv <- data.frame(start = 0, end = Inf)
  for (p in params) iv[[p]] <- TRUE
  opts <- modifyList(base_options, option_overrides)
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

versions <- list(
  R = R.version.string,
  PKNCA = as.character(packageVersion("PKNCA")),
  jsonlite = as.character(packageVersion("jsonlite"))
)

for (cs in cases) {
  overrides <- if (is.null(cs$options)) list() else cs$options
  res <- build_case(cs$profile, cs$route, cs$auc_method, cs$params, overrides)
  res$parameter <- factor(res$parameter, levels = cs$params)
  res <- res[order(res$subject, res$parameter), ]
  res$parameter <- as.character(res$parameter)
  subjects <- sort(unique(cs$profile$subject))
  stopifnot(nrow(res) == length(subjects) * length(cs$params))

  write_lines_lf(c("subject,parameter,value",
                   sprintf("%d,%s,%s", res$subject, res$parameter, num(res$value))),
                 file.path(exp_dir, paste0(cs$name, ".csv")))

  opts <- modifyList(base_options, overrides)
  opts$auc.method <- cs$auc_method
  meta <- list(
    schema = 1L,
    case = cs$name,
    dataset = cs$dataset,
    data_file = paste0("data/", cs$dataset, ".csv"),
    generated_by = "oracle/scripts/nca_pknca.R",
    route = if (cs$route == "extravascular") "extravascular" else "iv_bolus",
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
    preprocessing = if (cs$route == "extravascular") {
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
  write_lines_lf(toJSON(meta, pretty = TRUE, auto_unbox = TRUE, digits = NA, null = "null"),
                 file.path(exp_dir, paste0(cs$name, ".options.json")))
  cat(sprintf("%-16s %3d subjects, %4d values (%d NA)\n", cs$name, length(subjects), nrow(res), sum(is.na(res$value))))
}
cat(sprintf("%s | PKNCA %s | jsonlite %s\n", versions$R, versions$PKNCA, versions$jsonlite))
