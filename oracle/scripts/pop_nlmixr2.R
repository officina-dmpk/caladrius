# Caladrius population-fit oracle, card T-152 item 6: the nlmixr2 reference (POP-ORA-02, POP-ORA-03).
#
# WRITTEN, NOT RUN. nlmixr2, rxode2 and Rtools are not installed (Q-021) and this card installs none of them.
# The script is checked for syntax only (parse). Every statement about what nlmixr2 does below comes from
# specs/population.md POP-ORA-02 (the documentation read there: nlmixr2est 7.2.1) and is `documented, untested`
# until the tool is installed and the script has been run; the first run must record the versions it finds.
#
# What it does, per case of oracle/expected/pop/ofv/ (the same parameters, the same data):
#   1. the objective AT the given parameters, with nothing estimated: every parameter is fixed in `ini()`, and
#      the route is the documented `posthoc` method with `interaction = TRUE` (FOCEI). The control page of
#      `posthoc` says maxOuterIterations is ignored and set to 0; its `interaction` default is FALSE, so TRUE
#      must be passed or the result is FOCE.
#   2. the second route (an inference, documented-untested): est = "focei" with
#      foceiControl(maxOuterIterations = 0, covMethod = "", interaction = TRUE).
#   3. FOCE (interaction = FALSE) and FO (fo = TRUE) on the first route, for the oracle's `fo` and the frozen-sigma cases.
#   Each is run at two values of `sigdig` (3 and 6) so that the reference's own numerical noise is measured (the
#   closed form of linCmt() removes the ODE error; the setting still moves the inner optimiser's stopping point).
# Recorded per run: OBJF (the constant omitted), -2 logLik with adjLik = TRUE, their difference (the test of
# POP-ORA-02 is that it equals N ln 2pi), the eta at the conditional modes, and the versions of nlmixr2,
# nlmixr2est and rxode2. The oracle's own values (laplace_full, laplace_expected, linearised, fo, no_2pi) from
# the case file are copied next to them so that the comparison is a diff of one file.
#
# Run (when the tools exist): Rscript oracle/scripts/pop_nlmixr2.R   (writes oracle/expected/pop/second_look/nlmixr2.json)
suppressMessages({ library(jsonlite); library(nlmixr2) })
src <- sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)[1])
source(file.path(dirname(src), "pop_second_lib.R"))
ROOT <- repo_root2()
OUT <- file.path(ROOT, "oracle", "expected", "pop", "second_look", "nlmixr2.json")
dir.create(dirname(OUT), recursive = TRUE, showWarnings = FALSE)

ALL_IDS <- c(sprintf("syn_%s_%s", rep(c("iv", "oral", "pk2"), each = 4), rep(c("additive", "proportional", "combined", "combined_sd"), 3)),
             sprintf("theoph_%s", c("additive", "proportional", "combined", "combined_sd")),
             sprintf("indometh_%s", c("additive", "proportional", "combined", "combined_sd")))

# the model text: closed-form linCmt(), parameters fixed at the case values, random effects at Omega
model_function <- function(cs) {
  pars <- PARAMS[[cs$model]]
  lines <- character(); add <- function(...) lines <<- c(lines, paste0(...))
  add("function() {")
  add("  ini({")
  for (p in pars) add("    l", p, " <- fix(", format(log(cs$theta[[p]]), digits = 17), ")")
  q <- length(cs$re)
  if (omega_is_block(cs$omega)) {
    # nlmixr2 block syntax: eta.a + eta.b ~ c(var_a, cov_ab, var_b), lower triangle by rows
    vals <- c(); for (i in seq_len(q)) for (k in seq_len(i)) vals <- c(vals, format(cs$omega[i, k], digits = 17))
    add("    ", paste0("eta.", cs$re, collapse = " + "), " ~ fix(c(", paste(vals, collapse = ", "), "))")
  } else {
    for (i in seq_len(q)) add("    eta.", cs$re[i], " ~ fix(", format(cs$omega[i, i], digits = 17), ")")
  }
  if (cs$error_model %in% c("additive", "combined", "combined_sd")) add("    add.sd <- fix(", format(cs$xi$a, digits = 17), ")")
  if (cs$error_model %in% c("proportional", "combined", "combined_sd")) add("    prop.sd <- fix(", format(cs$xi$b, digits = 17), ")")
  add("  })")
  add("  model({")
  for (p in pars) add("    ", p, " <- exp(l", p, if (p %in% cs$re) paste0(" + eta.", p) else "", ")")
  add("    cp <- linCmt()")
  add("    cp ~ ", switch(cs$error_model, additive = "add(add.sd)", proportional = "prop(prop.sd)",
                         combined = "add(add.sd) + prop(prop.sd)", combined_sd = "add(add.sd) + prop(prop.sd) + combined1()"))
  add("  })")
  add("}")
  eval(parse(text = paste(lines, collapse = "\n")), envir = globalenv())
}

event_table <- function(cs) {
  d <- cs$data; ids <- unique(d$id)
  do.call(rbind, lapply(seq_along(ids), function(i) {
    s <- d[d$id == ids[i], ]
    rbind(data.frame(ID = i, TIME = 0, EVID = 1, AMT = s$dose[1], DV = NA_real_),
          data.frame(ID = i, TIME = s$time, EVID = 0, AMT = NA_real_, DV = s$y))
  }))
}

routes <- list(
  list(name = "posthoc_focei", est = "posthoc", control = function(sd) posthocControl(interaction = TRUE, sigdig = sd, adjLik = TRUE)),
  list(name = "focei_zero_outer", est = "focei", control = function(sd) foceiControl(maxOuterIterations = 0, covMethod = "", interaction = TRUE, sigdig = sd, adjLik = TRUE)),
  list(name = "posthoc_foce", est = "posthoc", control = function(sd) posthocControl(interaction = FALSE, sigdig = sd, adjLik = TRUE)),
  list(name = "posthoc_fo", est = "posthoc", control = function(sd) posthocControl(interaction = FALSE, fo = TRUE, sigdig = sd, adjLik = TRUE)))

run_route <- function(cs, f, dat, route, sigdig) {
  fit <- tryCatch(nlmixr2(f, dat, est = route$est, control = route$control(sigdig)), error = function(e) conditionMessage(e))
  if (is.character(fit)) return(list(route = route$name, sigdig = sigdig, error = fit))
  n <- sum(dat$EVID == 0)
  objf <- as.numeric(fit$objf)
  m2ll <- -2 * as.numeric(logLik(fit))
  list(route = route$name, sigdig = sigdig, objf = objf, minus2ll_adjlik = m2ll, difference = m2ll - objf,
       n_ln_2pi = n * log(2 * pi), eta = lapply(seq_len(nrow(fit$eta)), function(i) as.list(unlist(fit$eta[i, -1, drop = FALSE]))))
}

cases <- list()
for (id in ALL_IDS) {
  p <- file.path(ROOT, "oracle", "expected", "pop", "ofv", paste0(id, ".json"))
  if (!file.exists(p)) { cat("missing case", id, "\n"); next }
  cs <- load_ofv_case(ROOT, id)
  j <- fromJSON(p, simplifyVector = FALSE)
  f <- model_function(cs); dat <- event_table(cs)
  runs <- list()
  for (r in routes) for (sd in c(3, 6)) runs[[length(runs) + 1L]] <- run_route(cs, f, dat, r, sd)
  cases[[length(cases) + 1L]] <- list(id = id, model = cs$model, error_model = cs$error_model, runs = runs,
                                      oracle_total_full = j$expected$total_full, oracle_total_no_2pi = j$expected$total_no_2pi)
  cat(id, "\n")
}
out <- list(schema = 1L, kind = "pop_reference_nlmixr2", informational = FALSE, generated_by = "oracle/scripts/pop_nlmixr2.R",
            versions = list(R = R.version.string, nlmixr2 = as.character(packageVersion("nlmixr2")), nlmixr2est = as.character(packageVersion("nlmixr2est")),
                            rxode2 = as.character(packageVersion("rxode2")), jsonlite = as.character(packageVersion("jsonlite"))),
            cases = cases)
con <- file(OUT, open = "wb"); writeLines(toJSON(out, auto_unbox = TRUE, pretty = TRUE, digits = NA, null = "null", na = "null"), con, sep = "\n", useBytes = TRUE); close(con)
cat("pop_nlmixr2: ", length(cases), " cases; sha256 ", digest::digest(file = OUT, algo = "sha256", serialize = FALSE), "\n", sep = "")
