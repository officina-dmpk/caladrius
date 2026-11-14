# Caladrius population-fit oracle, card T-152 item 6: shared helpers of the SECOND-LOOK scripts
# (pop_nlme.R, pop_saemix.R) and of the written-but-not-run reference script (pop_nlmixr2.R).
# Plain double precision; no Rmpfr. The cases are those of oracle/expected/pop/ofv/ (items 2 and 3).
suppressMessages(library(jsonlite))

repo_root2 <- function() {
  a <- commandArgs(FALSE); f <- sub("^--file=", "", a[grepl("^--file=", a)])
  if (length(f) == 1L) normalizePath(file.path(dirname(f), "..", ".."), winslash = "/", mustWork = TRUE) else normalizePath(".", winslash = "/", mustWork = TRUE)
}

# One case of oracle/expected/pop/ofv/<id>.json as plain numbers.
load_ofv_case <- function(root, id) {
  j <- fromJSON(file.path(root, "oracle", "expected", "pop", "ofv", paste0(id, ".json")), simplifyVector = FALSE)
  m <- j$model
  q <- length(m$random_effects)
  omega <- matrix(0, q, q)
  for (i in seq_len(q)) for (k in seq_len(q)) omega[i, k] <- as.numeric(m$omega[[i]][[k]])
  rows <- do.call(rbind, lapply(j$subjects, function(s)
    data.frame(id = s$id, time = as.numeric(unlist(s$time)), y = as.numeric(unlist(s$y)), dose = as.numeric(s$dose), stringsAsFactors = FALSE)))
  list(id = j$id, model = m$name, re = unlist(m$random_effects), theta = lapply(m$theta, as.numeric),
       omega = omega, error_model = m$error_model, xi = lapply(m$xi, as.numeric), data = rows)
}
omega_is_block <- function(om) any(om[row(om) != col(om)] != 0)

# Closed forms in double precision (the clearance parametrisation of crates/caladrius-models), natural scale.
pred_iv_bolus <- function(time, dose, v, cl) (dose / v) * exp(-(cl / v) * time)
pred_oral_1 <- function(time, dose, v, cl, ka) { k <- cl / v; (dose * ka / (v * (ka - k))) * (exp(-k * time) - exp(-ka * time)) }
pred_pk2_iv_bolus <- function(time, dose, cl, vc, q, vp) {
  k10 <- cl / vc; k12 <- q / vc; k21 <- q / vp
  s <- k10 + k12 + k21; r <- sqrt((k10 - k21)^2 + k12 * (k12 + 2 * k10 + 2 * k21)); al <- (s + r) / 2; be <- (k10 * k21) / al
  (dose / vc) * (((al - k21) / (al - be)) * exp(-al * time) + ((k21 - be) / (al - be)) * exp(-be * time))
}
PARAMS <- list(`pk1.iv_bolus` = c("v", "cl"), `pk1.oral_1` = c("v", "cl", "ka"), `pk2.iv_bolus` = c("cl", "vc", "q", "vp"))
PRED_FN <- list(`pk1.iv_bolus` = "pred_iv_bolus", `pk1.oral_1` = "pred_oral_1", `pk2.iv_bolus` = "pred_pk2_iv_bolus")

# The cases the second looks run on: additive and proportional error (the error models the two tools offer by
# name), every synthetic, Theoph and Indometh case.
second_look_ids <- function() {
  models <- c("iv", "oral", "pk2")
  c(sprintf("syn_%s_%s", rep(models, each = 2), rep(c("additive", "proportional"), 3)),
    "theoph_additive", "theoph_proportional", "indometh_additive", "indometh_proportional")
}
fnum <- function(x) if (is.null(x)) NA_real_ else as.numeric(x)
