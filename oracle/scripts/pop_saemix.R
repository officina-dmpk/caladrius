# Caladrius population-fit oracle, card T-152 item 6: saemix as a SECOND LOOK (POP-ORA-03).
#
# INFORMATIONAL: nothing in this file is a pass/fail reference. saemix is a stochastic-approximation EM, a
# different estimator from the Laplace/FOCE family: its estimates differ from them by the difference of the
# two approximations and by the random stream; it is used to confirm the region of the optimum, not to fix a
# digit. The seed is fixed (saemixControl seed), so the output is reproducible on this machine. Log-normal
# random effects (transform.par = 1); error model "constant" (additive) or "proportional"; started at the
# parameters of the case. The cases are those of oracle/expected/pop/ofv/ (run pop_ofv.R first).
# saemix was installed with install.packages from CRAN for this card (and the packages it depends on).
#
# Run: Rscript oracle/scripts/pop_saemix.R   (writes oracle/expected/pop/second_look/saemix.json)
suppressMessages(library(saemix))
src <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)[1])
source(file.path(dirname(src), "pop_second_lib.R"))
ROOT <- repo_root2()
OUT <- file.path(ROOT, "oracle", "expected", "pop", "second_look", "saemix.json")
dir.create(dirname(OUT), recursive = TRUE, showWarnings = FALSE)
SEED <- 632545L

# the structural model for saemix: psi[id, j] are the individual parameters on the natural scale
sx_models <- list(
  `pk1.iv_bolus` = function(psi, id, xidep) pred_iv_bolus(xidep[, 1], xidep[, 2], psi[id, 1], psi[id, 2]),
  `pk1.oral_1` = function(psi, id, xidep) pred_oral_1(xidep[, 1], xidep[, 2], psi[id, 1], psi[id, 2], psi[id, 3]),
  `pk2.iv_bolus` = function(psi, id, xidep) pred_pk2_iv_bolus(xidep[, 1], xidep[, 2], psi[id, 1], psi[id, 2], psi[id, 3], psi[id, 4]))

slot_or_na <- function(obj, name) tryCatch({ v <- slot(obj, name); if (length(v) == 0) NA_real_ else as.numeric(v)[1] }, error = function(e) NA_real_)

run_case <- function(id) {
  cs <- load_ofv_case(ROOT, id)
  pars <- PARAMS[[cs$model]]; np <- length(pars)
  d <- cs$data; d$id <- as.integer(factor(d$id, levels = unique(d$id)))
  has_re <- pars %in% cs$re
  cov_model <- matrix(0, np, np)
  re_idx <- match(cs$re, pars)
  if (omega_is_block(cs$omega)) cov_model[re_idx, re_idx] <- 1 else for (i in re_idx) cov_model[i, i] <- 1
  omega_init <- diag(np)
  for (a in seq_along(re_idx)) for (b in seq_along(re_idx)) omega_init[re_idx[a], re_idx[b]] <- cs$omega[a, b]
  psi0 <- matrix(vapply(pars, function(p) cs$theta[[p]], 0), nrow = 1, dimnames = list(NULL, pars))
  sd_ <- saemixData(name.data = d, name.group = "id", name.predictors = c("time", "dose"), name.response = "y", verbose = FALSE)
  em <- if (cs$error_model == "additive") "constant" else "proportional"
  err_init <- if (cs$error_model == "additive") c(cs$xi$a, 1) else c(1, cs$xi$b)
  sm <- saemixModel(model = sx_models[[cs$model]], psi0 = psi0, transform.par = rep(1, np), covariance.model = cov_model,
                    omega.init = omega_init, error.model = em, verbose = FALSE)
  opt <- saemixControl(seed = SEED, map = TRUE, fim = TRUE, ll.is = TRUE, ll.gq = FALSE, nbiter.saemix = c(300, 100),
                       displayProgress = FALSE, save = FALSE, save.graphs = FALSE, print = FALSE, warnings = FALSE)
  res <- list(id = id, model = cs$model, error_model = cs$error_model, random_effects = as.list(cs$re), n_obs = nrow(d), seed = SEED)
  fit <- tryCatch(suppressWarnings(saemix(sm, sd_, opt)), error = function(e) conditionMessage(e))
  if (is.character(fit)) { res$converged <- FALSE; res$error <- fit; return(res) }
  r <- fit@results
  res$converged <- TRUE
  res$theta_hat <- as.list(setNames(as.numeric(r@fixed.effects), pars))
  om <- r@omega[re_idx, re_idx, drop = FALSE]
  res$omega_hat <- lapply(seq_len(nrow(om)), function(i) as.list(unname(om[i, ])))
  res$respar <- as.list(as.numeric(r@respar))
  res$minus2ll_is <- -2 * slot_or_na(r, "ll.is")
  res$minus2ll_lin <- -2 * slot_or_na(r, "ll.lin")
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
out <- list(schema = 1L, kind = "pop_second_look_saemix", informational = TRUE, generated_by = "oracle/scripts/pop_saemix.R",
            versions = list(R = R.version.string, saemix = as.character(packageVersion("saemix")), jsonlite = as.character(packageVersion("jsonlite")),
                            RNGkind = paste(RNGkind(), collapse = "/"), seed = SEED),
            note = "second look, never pass/fail: stochastic-approximation EM estimates (typical values on the natural scale, Omega of the log-normal effects, respar = (a, b) of saemix), started at the parameters of the case; the minus2ll values are saemix's importance-sampling and linearisation approximations, with their own constant convention",
            cases = cases)
con <- file(OUT, open = "wb"); writeLines(toJSON(out, auto_unbox = TRUE, pretty = TRUE, digits = NA, null = "null", na = "null"), con, sep = "\n", useBytes = TRUE); close(con)
cat("pop_saemix: ", length(cases), " cases; sha256 ", digest::digest(file = OUT, algo = "sha256", serialize = FALSE), "\n", sep = "")
