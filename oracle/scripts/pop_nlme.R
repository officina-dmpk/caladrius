# Caladrius population-fit oracle, card T-152 item 6: nlme::nlme as a SECOND LOOK (POP-ORA-03).
#
# INFORMATIONAL: nothing in this file is a pass/fail reference. nlme maximises the Lindstrom-Bates
# approximation of the likelihood (method = "ML"), with random effects additive on the parameter; the
# log-normal structure is obtained by writing the model on the log scale, so the fixed effects it reports are
# ln(theta) and Omega is the variance of the log-scale effects. Error: additive (no variance function) and
# proportional (varPower with the power fixed at 1, the scale estimated). Started at the parameters of the case.
# The cases are those of oracle/expected/pop/ofv/ (they must exist: run pop_ofv.R first).
#
# Run: Rscript oracle/scripts/pop_nlme.R   (writes oracle/expected/pop/second_look/nlme.json)
suppressMessages(library(nlme))
src <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)[1])
source(file.path(dirname(src), "pop_second_lib.R"))
ROOT <- repo_root2()
OUT <- file.path(ROOT, "oracle", "expected", "pop", "second_look", "nlme.json")
dir.create(dirname(OUT), recursive = TRUE, showWarnings = FALSE)

# the model on the log scale, one function per structural model (arguments: time, dose, then ln of each parameter)
nl_iv <- function(time, dose, lv, lcl) pred_iv_bolus(time, dose, exp(lv), exp(lcl))
nl_oral <- function(time, dose, lv, lcl, lka) pred_oral_1(time, dose, exp(lv), exp(lcl), exp(lka))
nl_pk2 <- function(time, dose, lcl, lvc, lq, lvp) pred_pk2_iv_bolus(time, dose, exp(lcl), exp(lvc), exp(lq), exp(lvp))
NL_FN <- list(`pk1.iv_bolus` = "nl_iv", `pk1.oral_1` = "nl_oral", `pk2.iv_bolus` = "nl_pk2")

run_case <- function(id) {
  cs <- load_ofv_case(ROOT, id)
  pars <- PARAMS[[cs$model]]; lnames <- paste0("l", pars)
  d <- cs$data; d$id <- factor(d$id, levels = unique(d$id))
  start <- setNames(vapply(pars, function(p) log(cs$theta[[p]]), 0), lnames)
  re_l <- paste0("l", cs$re)
  rand <- if (omega_is_block(cs$omega)) pdSymm(as.formula(paste(paste(re_l, collapse = " + "), "~ 1")))
          else pdDiag(as.formula(paste(paste(re_l, collapse = " + "), "~ 1")))
  form <- as.formula(sprintf("y ~ %s(time, dose, %s)", NL_FN[[cs$model]], paste(lnames, collapse = ", ")))
  fx <- as.formula(paste(paste(lnames, collapse = " + "), "~ 1"))
  wts <- if (cs$error_model == "proportional") varPower(fixed = 1) else NULL
  res <- list(id = id, model = cs$model, error_model = cs$error_model, random_effects = as.list(cs$re), n_obs = nrow(d),
              start_log_theta = as.list(start))
  fit <- tryCatch(
    nlme(form, data = d, fixed = fx, random = rand, groups = ~ id, start = start, weights = wts, method = "ML",
         control = nlmeControl(maxIter = 200, pnlsMaxIter = 20, msMaxIter = 200, tolerance = 1e-8, returnObject = TRUE)),
    error = function(e) conditionMessage(e))
  if (is.character(fit)) { res$converged <- FALSE; res$error <- fit; return(res) }
  om <- as.matrix(fit$modelStruct$reStruct)[[1]] * fit$sigma^2
  res$converged <- TRUE
  res$iterations <- as.integer(fit$numIter)
  res$minus2ll <- -2 * as.numeric(logLik(fit))
  res$fixed_log_theta <- as.list(fixef(fit))
  res$theta_hat <- as.list(setNames(exp(fixef(fit)), sub("^l", "", names(fixef(fit)))))
  res$omega_hat <- lapply(seq_len(nrow(om)), function(i) as.list(unname(om[i, ])))
  res$omega_names <- as.list(rownames(om))
  res$sigma_hat <- as.numeric(fit$sigma)
  res
}

ids <- second_look_ids()
cases <- list()
for (id in ids) {
  if (!file.exists(file.path(ROOT, "oracle", "expected", "pop", "ofv", paste0(id, ".json")))) { cat("missing case", id, "\n"); next }
  t0 <- proc.time()[[3]]
  cases[[length(cases) + 1L]] <- run_case(id)
  cat(sprintf("%-26s %s %.1fs\n", id, if (isTRUE(cases[[length(cases)]]$converged)) "ok" else "FAILED", proc.time()[[3]] - t0))
}
out <- list(schema = 1L, kind = "pop_second_look_nlme", informational = TRUE, generated_by = "oracle/scripts/pop_nlme.R",
            versions = list(R = R.version.string, nlme = as.character(packageVersion("nlme")), jsonlite = as.character(packageVersion("jsonlite"))),
            note = "second look, never pass/fail: Lindstrom-Bates ML estimates on the log scale (fixed effects are ln theta, Omega is the variance of the log-scale effects), started at the parameters of the case; minus2ll is nlme's own approximation, without the constant convention of any other tool",
            cases = cases)
con <- file(OUT, open = "wb"); writeLines(toJSON(out, auto_unbox = TRUE, pretty = TRUE, digits = NA, null = "null", na = "null"), con, sep = "\n", useBytes = TRUE); close(con)
cat("pop_nlme: ", length(cases), " cases; sha256 ", digest::digest(file = OUT, algo = "sha256", serialize = FALSE), "\n", sep = "")
