# Caladrius population-fit oracle, card T-152 item 5 (POP-ORA-04): SEALED simulate-then-fit data.
#
# K replicate datasets are simulated from KNOWN true parameters, written one file per replicate
# (subject,time,conc,dose; the profile layout of oracle/data/theoph.csv), with the SHA-256 of each
# file and a truth file that holds the true values, the design and the rule of the aggregate
# checks. NO FIT IS RUN HERE. The files are committed before any engine commit and are never edited,
# regenerated with other seeds or re-rolled: the test recomputes every hash.
#
# Two designs (OP-13):
#   accurate  pk1.oral_1, random effects on cl, v, ka (diagonal Omega), combined error, 40 subjects x 8
#             observations: where the Laplace/FOCE estimate is expected to be accurate. The aggregate
#             checks (bias of the mean, coverage) are ASSERTED here.
#   limits    pk1.oral_1, random effects on cl and v with a large Omega, 30 subjects x 3 observations:
#             the approximation is known to be strained (sparse data, large variances). The aggregates
#             are RECORDED, not asserted (a question for Abdou in the report).
#
# Run (ONCE, already done; the script now stops when its outputs exist): Rscript oracle/scripts/pop_sealed.R   (writes oracle/data/pop/sealed/ and oracle/expected/pop/sealed/)
source(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)[1])), "pop_lib.R"))
ROOT <- repo_root()
DATA <- file.path(ROOT, "oracle", "data", "pop", "sealed")
EXP <- file.path(ROOT, "oracle", "expected", "pop", "sealed")
# GUARD (T-152 fix round A): the sealed data are never regenerated. If any output exists the script stops here,
# before the first unlink; there is no option to override it. Removing the sealed files by hand is a decision
# of Abdou, not of a script.
existing <- c(list.files(DATA, recursive = TRUE), list.files(EXP, recursive = TRUE))
if (length(existing) > 0L) {
  stop("pop_sealed.R: the sealed outputs already exist (", length(existing), " files under oracle/data/pop/sealed and ",
       "oracle/expected/pop/sealed). Sealed data are never regenerated, re-rolled or edited (POP-ORA-04); nothing was ",
       "deleted. Only Abdou may decide to remove them by hand.", call. = FALSE)
}
unlink(DATA, recursive = TRUE); unlink(EXP, recursive = TRUE)
dir.create(DATA, recursive = TRUE, showWarnings = FALSE); dir.create(EXP, recursive = TRUE, showWarnings = FALSE)
arr <- function(x) as.list(x)

K_REPLICATES <- 200L
C_BIAS <- 3        # bias band: |mean(estimate) - truth| <= C_BIAS * sd(estimate) / sqrt(K)
C_INTERVAL <- 1.96 # Wald interval: estimate +/- C_INTERVAL * se
C_COVERAGE <- 3    # coverage band: |coverage - 0.95| <= C_COVERAGE * sqrt(0.95 * 0.05 / K)

designs <- list(
  accurate = list(
    base_seed = 15200000L, m = 40L, times = c("0.25", "0.5", "1", "2", "4", "6", "8", "12"),
    model = "pk1.oral_1", theta = list(v = "50", cl = "5", ka = "1.5"), re = c("cl", "v", "ka"),
    omega = list(list("0.09", "0", "0"), list("0", "0.04", "0"), list("0", "0", "0.16")),
    error_model = "combined", xi = list(a = "0.02", b = "0.1"), dose = "100", assert = TRUE,
    description = "accurate design: 40 subjects x 8 observations, three random effects, combined error; aggregates asserted"),
  limits = list(
    base_seed = 15300000L, m = 30L, times = c("0.5", "1.5", "4"),
    model = "pk1.oral_1", theta = list(v = "50", cl = "5", ka = "1.5"), re = c("cl", "v"),
    omega = list(list("0.36", "0"), list("0", "0.25")),
    error_model = "combined", xi = list(a = "0.02", b = "0.15"), dose = "100", assert = FALSE,
    description = "limits design: 30 subjects x 3 observations, large Omega, combined error; aggregates recorded, not asserted"))

for (name in names(designs)) {
  d <- designs[[name]]; ddir <- file.path(DATA, name); dir.create(ddir, recursive = TRUE, showWarnings = FALSE)
  files <- vector("list", K_REPLICATES)
  for (k in seq_len(K_REPLICATES)) {
    subj <- simulate_subjects(d$model, d$theta, d$re, d$omega, d$error_model, d$xi, d$dose, d$times, d$m, d$base_seed + k, sig = 4, prefix = "")
    rows <- unlist(lapply(subj, function(s) paste(s$id, s$time, s$y, s$dose, sep = ",")))
    ys <- as.numeric(unlist(lapply(subj, function(s) s$y)))
    if (any(!is.finite(ys)) || any(ys <= 0)) stop("design ", name, " replicate ", k, ": a non-positive concentration; the design must be changed BEFORE sealing")
    path <- file.path(ddir, sprintf("rep_%03d.csv", k))
    write_lines_lf(c("subject,time,conc,dose", rows), path)
    files[[k]] <- list(replicate = k, seed = d$base_seed + k, path = sprintf("data/pop/sealed/%s/rep_%03d.csv", name, k),
                       rows = length(rows), sha256 = sha256_file(path))
  }
  man <- paste(vapply(files, function(f) paste(f$path, f$sha256), ""), collapse = "\n")
  manifest <- digest::digest(paste0(man, "\n"), algo = "sha256", serialize = FALSE)
  truth <- list(
    schema = 1L, kind = "pop_sealed", name = name, description = d$description, sealed = TRUE,
    rule = "committed before any engine commit; never edited, regenerated or re-rolled; the test recomputes every hash below",
    generated_by = "oracle/scripts/pop_sealed.R", versions = versions_list(),
    generation = list(seed_rule = "set.seed(base_seed + k, kind = Mersenne-Twister, normal.kind = Inversion, sample.kind = Rejection) for replicate k = 1..K",
                      base_seed = d$base_seed, k = K_REPLICATES, significant_digits = 4L,
                      individual = "eta ~ N(0, Omega) by the transposed Cholesky factor; P_i = theta * exp(eta) on the listed parameters",
                      residual = "y = yhat + sd * N(0, 1), sd = sqrt(a^2 + b^2 yhat^2) (combined)"),
    model = list(name = d$model, theta = d$theta, random_effects = arr(d$re), omega = lapply(d$omega, arr),
                 error_model = d$error_model, xi = d$xi, dose = d$dose),
    design = list(subjects = d$m, times = arr(d$times), observations_per_subject = length(d$times),
                  observations_per_replicate = d$m * length(d$times), replicates = K_REPLICATES),
    truth = list(typical_values = d$theta,
                 omega_diagonal = setNames(arr(vapply(seq_along(d$re), function(i) d$omega[[i]][[i]], "")), d$re),
                 omega_full = lapply(d$omega, arr), xi = d$xi),
    aggregate_checks = list(
      asserted = d$assert,
      note = if (d$assert) "asserted on the typical values (theta); recorded for Omega and xi" else "all aggregates recorded, none asserted",
      constants = list(c_bias = C_BIAS, c_interval = C_INTERVAL, c_coverage = C_COVERAGE, nominal_coverage = 0.95),
      bias = "for each typical value: |mean over replicates of the estimate - truth| <= c_bias * sd(estimates) / sqrt(K)",
      coverage = "for each typical value: share of replicates with truth in estimate +/- c_interval * se, |share - 0.95| <= c_coverage * sqrt(0.95 * 0.05 / K)",
      per_replicate = "deviations of each replicate are recorded by the test, never asserted",
      converged = "a replicate whose fit is not converged is counted and reported, and excluded from the aggregates"),
    files = files, manifest_sha256 = manifest)
  write_json_lf(truth, file.path(EXP, paste0(name, ".json")))
  cat(sprintf("%-10s K=%d  files=%d  manifest=%s\n", name, K_REPLICATES, length(files), manifest))
}
