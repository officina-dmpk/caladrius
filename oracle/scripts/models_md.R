#!/usr/bin/env Rscript
# Caladrius multiple-dosing and steady-state oracle (task T-047, open item OM-16).
#
# Ground truth for specs/models.md section 12 (MOD-MD-01..12) before any engine code exists, for the
# twelve ids pk1.* and pk2.* (iv_bolus, iv_infusion, oral_1, oral_1_lag, oral_0, oral_0_lag):
#   - superposition: a schedule of doses (time, amount, duration), any order, overlaps allowed
#     (MOD-MD-01), and regular regimens of n equal doses (MOD-MD-07), n = 1, 3, 10, 40;
#   - steady state of a regular regimen of interval tau (MOD-MD-03 to 06): the profile over one
#     interval, the derived quantities cmax_ss, tmax_ss, cmin_ss, cav_ss, auc_tau_ss, accum_cmax,
#     accum_auc and the per-time ratio accum_c (MOD-MD-08 to 10);
#   - the two convention points of OM-17 (e) and (f) as explicit cases (group "convention");
#   - model_md_errors: what the engine must refuse (MOD-MD-12), as not-available rows with reasons.
#
# Independence from the engine and from the closed forms of the steady-state rules: every expected
# value is a plain sum of SINGLE-DOSE functions (the explicit sums of exponentials of MOD-IVB-01,
# MOD-IVI-01, MOD-AB1-01, MOD-AB0-01, MOD-2C-07..12 with the lag as a delay) evaluated at 256 bits
# (Rmpfr) and rounded once to double. Steady state is the sum over j = 0..N of f(s + j tau), N
# chosen so that the bounded tail is below 2^-70 of the smallest value of the case (checked); the
# geometric-series forms of MOD-MD-03/04/05/06/10 are NOT used to produce a value. They are
# implemented a second time in this script, in 256 bits, and every steady-state case is checked
# against them (agreement 2^-60), so that the closed forms of the specification are tested by the
# oracle and not the reverse. The areas are the exact integrals of the same terms.
#
# Every case is also cross-checked, within 1e-8 (relative to the value, values below 1e-3 of the
# largest one compared absolutely at 1e-11 of it), against an adaptive ODE solution (package
# deSolve, lsoda, rtol 1e-12) of the compartment equations with the doses applied as dosing events
# (amount added to the central or depot compartment, input rate switched on and off):
#   - schedules and regimens of n doses: the events of the schedule;
#   - steady state: (a) the periodic solution of the linear system, i.e. the fixed point of the
#     one-period map found by solving (I - M) x = b with M and b from the solver, then one period
#     of events from that state; (b) when the number of doses needed is at most 3000, a long train
#     of doses by events to convergence, read after the last dose.
# No random numbers are used; no timestamps are written; the output is byte for byte reproducible.
#
# Usage, from the repository root:
#   Rscript oracle/scripts/models_md.R
# Environment variable MD_OUT sends the output to another directory (reproducibility check). MD_ONLY (a regular expression) restricts the run to the matching case names,
# for development; it never writes the case list for the engine test in that mode.
# Writes into oracle/expected/models/md/:
#   model_md_<id>_<case>.csv           long table: subject,parameter,value (subject: t=<time> or scalar)
#   model_md_<id>_<case>.options.json  model id, dose, parameters, regimen, times, quantities, versions
#   model_md_errors.csv / .options.json
# and crates/caladrius-models/tests/oracle_models_md.cases (the list of cases the engine test runs).

suppressPackageStartupMessages({
  library(Rmpfr)
  library(deSolve)
})

args_all <- commandArgs(trailingOnly = FALSE)
file_arg <- sub("^--file=", "", args_all[grepl("^--file=", args_all)])
root <- if (length(file_arg) == 1L) {
  normalizePath(file.path(dirname(file_arg), "..", ".."), winslash = "/", mustWork = TRUE)
} else {
  normalizePath(".", winslash = "/", mustWork = TRUE)
}
exp_dir <- Sys.getenv("MD_OUT", unset = file.path(root, "oracle", "expected", "models", "md"))
dir.create(exp_dir, showWarnings = FALSE, recursive = TRUE)
ONLY <- Sys.getenv("MD_ONLY", unset = "")

write_lines_lf <- function(lines, path) {
  con <- file(path, open = "wb")
  on.exit(close(con))
  writeLines(lines, con, sep = "\n", useBytes = TRUE)
}
num17 <- function(x) ifelse(is.na(x), "", sprintf("%.17g", x))
# Shortest decimal text that reads back as the same double.
rt <- function(x) {
  s <- ""
  for (d in 15:17) {
    s <- sprintf(paste0("%.", d, "g"), x)
    if (as.numeric(s) == x) return(s)
  }
  s
}
`%||%` <- function(a, b) if (is.null(a)) b else a
check <- function(cond, ...) {
  if (!isTRUE(cond)) stop(paste0(..., collapse = ""), call. = FALSE)
}

# ---------------------------------------------------------------- minimal JSON writer
# jsonlite would print numbers with at most 15 significant digits; the parameters must read back as
# the very doubles the expected values were computed from.
jstr <- function(s) {
  s <- gsub("\\\\", "\\\\\\\\", s)
  s <- gsub("\"", "\\\\\"", s)
  paste0("\"", s, "\"")
}
jnum <- function(x) {
  if (is.na(x)) return("null")
  if (is.nan(x)) return("\"NaN\"")
  if (is.infinite(x)) return(if (x > 0) "\"Infinity\"" else "\"-Infinity\"")
  rt(x)
}
jenc <- function(x, ind = 0L) {
  pad <- strrep("  ", ind)
  pad1 <- strrep("  ", ind + 1L)
  if (is.null(x)) return("null")
  if (is.list(x)) {
    if (length(x) == 0L) return(if (is.null(names(x))) "[]" else "{}")
    if (!is.null(names(x))) {
      items <- mapply(function(k, v) paste0(pad1, jstr(k), ": ", jenc(v, ind + 1L)),
                      names(x), x, USE.NAMES = FALSE)
      return(paste0("{\n", paste(items, collapse = ",\n"), "\n", pad, "}"))
    }
    return(paste0("[", paste(vapply(x, jenc, "", ind = ind + 1L), collapse = ", "), "]"))
  }
  if (is.character(x)) {
    if (length(x) == 1L) return(jstr(x))
    return(paste0("[", paste(jstr(x), collapse = ", "), "]"))
  }
  if (is.logical(x)) {
    s <- ifelse(x, "true", "false")
    return(if (length(x) == 1L) s else paste0("[", paste(s, collapse = ", "), "]"))
  }
  if (is.numeric(x)) {
    s <- if (is.integer(x)) as.character(x) else vapply(x, jnum, "")
    return(if (length(x) == 1L) s else paste0("[", paste(s, collapse = ", "), "]"))
  }
  stop("jenc: unsupported value")
}
arr <- function(x) structure(as.list(x), class = NULL)

# ---------------------------------------------------------------- 256-bit helpers
PREC <- 256L
mp <- function(x) mpfr(x, PREC)
zero <- function() mp(0)
mzeros <- function(n) mpfr(rep(0, n), PREC)
TINY <- mp(2)^-150      # |ka - lambda| below this: the confluent limit
TAILBITS <- 70L          # the truncated tail of a steady-state sum is below 2^-TAILBITS
AGREE <- mp(2)^-60       # relative agreement of the closed forms of the specification with the sums
num <- function(x) if (is.null(x)) NA_real_ else as.numeric(x)

kind_of <- function(model) {
  if (grepl("iv_bolus$", model)) "bolus"
  else if (grepl("oral_1", model)) "first"
  else "zero"      # iv_infusion, oral_0, oral_0_lag
}
has_lag <- function(model) grepl("_lag$", model)
n_comp <- function(model) if (grepl("^pk1\\.", model)) 1L else 2L

# ---------------------------------------------------------------- model parameters (256 bits)
# pk1: v, cl.  pk2: clearance set (cl, vc, q, vp) or macro set (a, b, alpha, beta; the dose gives
# vc = D / (a + b)), as in oracle/scripts/models_2c_closed_form.R (T-032). A parameter set becomes
# a list of exponents `lam` with weights `w` (sum 1), the central volume `vc` and the micro-constants.
build_q <- function(model, set, p, D, extra) {
  P <- lapply(p, mp)
  Dm <- mp(D)
  if (n_comp(model) == 1L) {
    check(set == "clearance", "pk1 cases use the set (v, cl)")
    vc <- P[["v"]]
    cl <- P[["cl"]]
    k <- cl / vc
    q <- list(comp = 1L, lam = list(k), w = list(mp(1)), vc = vc, cl = cl,
              k10 = k, k12 = zero(), k21 = zero())
  } else {
    micro <- switch(set,
      clearance = list(vc = P[["vc"]], k10 = P[["cl"]] / P[["vc"]], k12 = P[["q"]] / P[["vc"]],
                       k21 = P[["q"]] / P[["vp"]]),
      macro = {
        s <- P[["a"]] + P[["b"]]
        wa0 <- P[["a"]] / s
        wb0 <- P[["b"]] / s
        k21 <- P[["alpha"]] * wb0 + P[["beta"]] * wa0
        k10 <- P[["alpha"]] * P[["beta"]] / k21
        list(vc = Dm / s, k10 = k10, k12 = P[["alpha"]] + P[["beta"]] - k10 - k21, k21 = k21)
      },
      stop("unknown set ", set))
    S <- micro$k10 + micro$k12 + micro$k21
    Pr <- micro$k10 * micro$k21
    r <- sqrt(S * S - 4 * Pr)
    alpha <- (S + r) / 2
    beta <- Pr / alpha
    q <- list(comp = 2L, lam = list(alpha, beta),
              w = list((alpha - micro$k21) / (alpha - beta), (micro$k21 - beta) / (alpha - beta)),
              vc = micro$vc, cl = micro$k10 * micro$vc,
              k10 = micro$k10, k12 = micro$k12, k21 = micro$k21)
  }
  q$ka <- if (!is.null(extra[["ka"]])) mp(extra[["ka"]]) else NULL
  q$tlag <- if (!is.null(extra[["tlag"]])) mp(extra[["tlag"]]) else zero()
  q$dur <- if (!is.null(extra[["dur"]])) mp(extra[["dur"]]) else NULL
  q$D <- Dm
  q
}

# ---------------------------------------------------------------- single-dose function
# A segment is a stretch of time [s0, s0 + len] (len NULL: to infinity) since the dose on which the
# concentration is a sum of terms c * u^p * exp(-lambda * u), u = t - s0 (the lag is the start s0 of
# the first segment: a delay, MOD-GEN-02).
trm <- function(c, l, p) list(c = c, l = l, p = as.integer(p))
sgm <- function(s0, len, terms) list(s0 = s0, len = len, terms = terms)

segments_for <- function(model, q, D, dur) {
  kd <- kind_of(model)
  D <- mp(D)
  coef <- lapply(q$w, function(w) D * w / q$vc)
  lam <- q$lam
  tl <- q$tlag
  if (kd == "bolus") {
    return(list(sgm(zero(), NULL, mapply(function(c, l) trm(c, l, 0L), coef, lam, SIMPLIFY = FALSE))))
  }
  if (kd == "zero") {
    Tt <- mp(dur)
    t1 <- list(); t2 <- list()
    for (i in seq_along(lam)) {
      cA <- coef[[i]] / (lam[[i]] * Tt)
      t1 <- c(t1, list(trm(cA, zero(), 0L), trm(-cA, lam[[i]], 0L)))
      t2 <- c(t2, list(trm(cA * (1 - exp(-lam[[i]] * Tt)), lam[[i]], 0L)))
    }
    return(list(sgm(tl, Tt, t1), sgm(tl + Tt, NULL, t2)))
  }
  ka <- q$ka
  terms <- list()
  for (i in seq_along(lam)) {
    if (abs(ka - lam[[i]]) < TINY) {
      terms <- c(terms, list(trm(coef[[i]] * ka, lam[[i]], 1L)))     # confluent limit
    } else {
      c1 <- coef[[i]] * ka / (ka - lam[[i]])
      terms <- c(terms, list(trm(c1, lam[[i]], 0L), trm(-c1, ka, 0L)))
    }
  }
  list(sgm(tl, NULL, terms))
}

upow <- function(u, p) if (p == 0L) 1 else u^p
eval_terms <- function(terms, u) {
  total <- 0 * u
  for (tm in terms) total <- total + tm$c * upow(u, tm$p) * exp(-tm$l * u)
  total
}
eval_dterms <- function(terms, u) {   # derivative with respect to u
  total <- 0 * u
  for (tm in terms) {
    d <- -tm$l * upow(u, tm$p)
    if (tm$p > 0L) d <- d + tm$p * upow(u, tm$p - 1L)
    total <- total + tm$c * d * exp(-tm$l * u)
  }
  total
}

# Concentration (or its derivative) at the times tv (mpfr vector, since the dose): 0 before the dose
# (bolus) and up to the start of the input otherwise (MOD-GEN-02); a bolus at exactly 0 counts.
seg_eval <- function(segs, tv, bolus, deriv = FALSE) {
  n <- length(tv)
  out <- mzeros(n)
  done <- rep(FALSE, n)
  for (sg in segs) {
    inside <- tv >= sg$s0
    if (!is.null(sg$len)) inside <- inside & (tv <= sg$s0 + sg$len)
    idx <- which(inside & !done)
    if (length(idx) == 0L) next
    u <- tv[idx] - sg$s0
    val <- if (deriv) eval_dterms(sg$terms, u) else eval_terms(sg$terms, u)
    if (!bolus && !deriv) val[u == 0] <- mp(0)
    out[idx] <- val
    done[idx] <- TRUE
  }
  out
}
conc_vec <- function(segs, tv, bolus) seg_eval(segs, tv, bolus, FALSE)
dconc_vec <- function(segs, tv, bolus) seg_eval(segs, tv, bolus, TRUE)

# integral over [0, x] of u^p * exp(-lam * u), x an mpfr vector
Ipow_vec <- function(p, lam, x) {
  if (lam == 0) return(x^(p + 1L) / (p + 1L))
  f <- mp(factorial(p))
  y <- lam * x
  s <- 0 * x
  for (j in 0:p) s <- s + upow(y, j) / factorial(j)
  f / lam^(p + 1L) * (1 - exp(-y) * s)
}
# AUC from the dose time to tv (mpfr vector), exact integral of the terms (0 before the dose).
area_vec <- function(segs, tv) {
  total <- 0 * tv
  for (sg in segs) {
    u <- tv - sg$s0
    u[u < 0] <- mp(0)
    if (!is.null(sg$len)) u[u > sg$len] <- sg$len
    for (tm in sg$terms) total <- total + tm$c * Ipow_vec(tm$p, tm$l, u)
  }
  total
}
area_inf <- function(segs) {
  total <- zero()
  for (sg in segs) {
    for (tm in sg$terms) {
      if (is.null(sg$len)) {
        total <- total + tm$c * mp(factorial(tm$p)) / tm$l^(tm$p + 1L)
      } else {
        total <- total + tm$c * Ipow_vec(tm$p, tm$l, mp(sg$len))
      }
    }
  }
  total
}

# ---------------------------------------------------------------- superposition (MOD-MD-01)
# recs: data frame (time, dose, dur) in the order given; tv: mpfr vector of times since the origin.
sched_eval <- function(model, q, recs, tv) {
  bolus <- kind_of(model) == "bolus"
  conc <- 0 * tv
  auc <- 0 * tv
  cache <- list()
  for (r in seq_len(nrow(recs))) {
    key <- paste(sprintf("%.17g", recs$dose[r]), sprintf("%.17g", recs$dur[r]))
    if (is.null(cache[[key]])) cache[[key]] <- segments_for(model, q, recs$dose[r], recs$dur[r])
    segs <- cache[[key]]
    u <- tv - mp(recs$time[r])
    conc <- conc + conc_vec(segs, u, bolus)
    auc <- auc + area_vec(segs, u)
  }
  list(conc = conc, auc = auc)
}

# ---------------------------------------------------------------- steady state as a plain sum
# C_ss(s) = sum over j >= 0 of f(s + j tau), s in [0, tau] (MOD-MD-03 definition).
lam_min_of <- function(segs) {
  sg <- segs[[length(segs)]]
  min(vapply(sg$terms, function(tm) as.numeric(tm$l), 0))
}
tail_bound <- function(segs, N, tau) {
  sg <- segs[[length(segs)]]
  b <- zero()
  for (tm in sg$terms) {
    r <- exp(-tm$l * tau)
    b <- b + abs(tm$c) * ((N + 2) * tau)^tm$p * exp(-tm$l * ((N + 1) * tau - sg$s0)) / (1 - r)^2
  }
  b
}
n_terms <- function(segs, tau, bits) {
  last <- segs[[length(segs)]]
  start <- as.numeric(last$s0)
  lm <- lam_min_of(segs)
  n0 <- ceiling((bits * log(2) + 12) / (lm * as.numeric(tau))) + ceiling(start / as.numeric(tau)) + 3
  max(5L, as.integer(n0))
}
ss_sum_direct <- function(segs, bolus, tau, s, N, deriv = FALSE) {
  tv <- s + mpfr(0:N, PREC) * tau
  if (deriv) sum(dconc_vec(segs, tv, bolus)) else sum(conc_vec(segs, tv, bolus))
}
ss_area_direct <- function(segs, tau, s, N) {
  j <- mpfr(0:N, PREC) * tau
  sum(area_vec(segs, s + j) - area_vec(segs, j))
}
# The same sums, factored to be affordable when the number of terms is large (a small interval): from
# the index J0 on, every dose is in the last (infinite) segment of its single-dose function, whose
# terms are c u^p exp(-lam u) with u = a + j tau, a = s - s0, so that exp(-lam u) = exp(-lam a)
# exp(-lam j tau) and the sum over j is exp(-lam a) times S0 = sum of exp(-lam j tau) (and S1 = sum of
# j tau exp(-lam j tau) for p = 1). S0 and S1 are summed term by term, j = J0 .. N, once for the case;
# they are NOT replaced by 1 / (1 - exp(-lam tau)). The first J0 terms are summed directly.
ss_prep <- function(segs, bolus, tau, N) {
  last <- segs[[length(segs)]]
  J0 <- max(1L, as.integer(ceiling(as.numeric(last$s0 / tau))) + 1L)
  N <- max(N, J0 + 2L)
  jv <- mpfr(J0:N, PREC)
  sums <- list()
  for (tm in last$terms) {
    known <- FALSE
    for (e in sums) if (e$lam == tm$l) known <- TRUE
    if (!known) {
      E <- exp(-tm$l * jv * tau)
      sums[[length(sums) + 1L]] <- list(lam = tm$l, S0 = sum(E), S1 = sum(jv * tau * E))
    }
  }
  list(segs = segs, bolus = bolus, tau = tau, N = N, J0 = J0, last = last, sums = sums)
}
prep_sums <- function(prep, lam) {
  for (e in prep$sums) if (e$lam == lam) return(e)
  stop("no sums for this exponent")
}
ss_eval <- function(prep, s, deriv = FALSE) {
  tv <- s + mpfr(0:(prep$J0 - 1L), PREC) * prep$tau
  head <- sum(if (deriv) dconc_vec(prep$segs, tv, prep$bolus) else conc_vec(prep$segs, tv, prep$bolus))
  a <- s - prep$last$s0
  tail <- zero()
  for (tm in prep$last$terms) {
    S <- prep_sums(prep, tm$l)
    ea <- exp(-tm$l * a)
    tail <- tail + if (!deriv) {
      tm$c * ea * (if (tm$p == 0L) S$S0 else a * S$S0 + S$S1)
    } else {
      tm$c * ea * (if (tm$p == 0L) -tm$l * S$S0 else S$S0 - tm$l * (a * S$S0 + S$S1))
    }
  }
  head + tail
}
# integral of the steady-state profile from 0 to s: sum over j of the integral of the single-dose
# function from j tau to j tau + s.
ss_area_p <- function(prep, s) {
  j <- mpfr(0:(prep$J0 - 1L), PREC) * prep$tau
  head <- sum(area_vec(prep$segs, s + j) - area_vec(prep$segs, j))
  s0 <- prep$last$s0
  tail <- zero()
  for (tm in prep$last$terms) {
    S <- prep_sums(prep, tm$l)
    l <- tm$l
    es <- exp(-l * s)
    tail <- tail + if (tm$p == 0L) {
      tm$c * exp(l * s0) * (1 - es) / l * S$S0
    } else {
      B <- S$S1 - s0 * S$S0
      tm$c * exp(l * s0) * ((B / l + S$S0 / l^2) - es * ((B + s * S$S0) / l + S$S0 / l^2))
    }
  }
  head + tail
}

# ---------------------------------------------------------------- closed forms of the specification
# MOD-MD-03 to 06, written a second time (unit volume, rate lam), used only to check the sums.
phi_ss <- function(kd, lam, D, Tt, ka, tau, u) {
  E <- exp(-lam * tau)
  if (kd == "bolus") return(D * exp(-lam * u) / (1 - E))
  if (kd == "zero") {
    R0 <- D / Tt
    if (u <= Tt) {
      return(R0 / lam * ((1 - exp(-lam * u)) + (1 - exp(-lam * Tt)) * exp(-lam * (u + tau - Tt)) / (1 - E)))
    }
    return(R0 / lam * (1 - exp(-lam * Tt)) * exp(-lam * (u - Tt)) / (1 - E))
  }
  if (abs(ka - lam) < TINY) {
    return(D * lam * exp(-lam * u) * (u / (1 - E) + tau * E / (1 - E)^2))
  }
  D * ka / (ka - lam) * (exp(-lam * u) / (1 - E) - exp(-ka * u) / (1 - exp(-ka * tau)))
}
ss_closed <- function(model, q, D, dur, tau, s) {
  kd <- kind_of(model)
  s <- mp(s)
  u <- if (kd == "bolus") s else {
    x <- s - q$tlag
    x - floor(x / tau) * tau
  }
  total <- zero()
  for (i in seq_along(q$lam)) {
    total <- total + q$w[[i]] / q$vc * phi_ss(kd, q$lam[[i]], mp(D), if (is.null(dur)) NULL else mp(dur), q$ka, tau, u)
  }
  total
}
# S-36's printed form for the interval before the end of the lag (p. 10 eq. 1.16, p. 14 eq. 1.26): the
# earlier dose is taken in its decay phase with the elapsed time s + tau - tlag.
s36_printed <- function(model, q, D, dur, tau, s) {
  kd <- kind_of(model)
  s <- mp(s)
  tl <- q$tlag
  if (s > tl) return(ss_closed(model, q, D, dur, tau, s))
  e <- s + tau - tl
  total <- zero()
  for (i in seq_along(q$lam)) {
    lam <- q$lam[[i]]
    E <- exp(-lam * tau)
    Dm <- mp(D)
    if (kd == "zero") {
      Tt <- mp(dur)
      ph <- Dm / (Tt * lam) * (1 - exp(-lam * Tt)) * exp(-lam * (e - Tt)) / (1 - E)
    } else {
      if (abs(q$ka - lam) < TINY) return(NULL)
      ph <- Dm * q$ka / (q$ka - lam) * (exp(-lam * e) / (1 - E) - exp(-q$ka * e) / (1 - exp(-q$ka * tau)))
    }
    total <- total + q$w[[i]] / q$vc * ph
  }
  total
}

# ---------------------------------------------------------------- single-dose peak and area
x_of <- function(ka, lam) if (abs(ka - lam) < TINY) 1 / lam else log(ka / lam) / (ka - lam)
single_peak <- function(model, q, D, dur) {
  kd <- kind_of(model)
  segs <- segments_for(model, q, D, dur)
  if (kd == "bolus") return(list(tmax = zero(), cmax = mp(D) / q$vc, segs = segs))
  if (kd == "zero") {
    t <- q$tlag + mp(dur)
    return(list(tmax = t, cmax = conc_vec(segs, t, FALSE), segs = segs))
  }
  xs <- lapply(q$lam, function(l) x_of(q$ka, l))
  lo <- xs[[1]]; hi <- xs[[length(xs)]]
  if (lo > hi) { tmp <- lo; lo <- hi; hi <- tmp }
  terms <- segs[[1]]$terms
  if (lo == hi) {
    u <- lo
  } else {
    sc <- abs(eval_dterms(terms, zero()))
    check(eval_dterms(terms, lo) >= -mp(2)^-100 * sc, "C' negative at the lower bracket")
    check(eval_dterms(terms, hi) <= mp(2)^-100 * sc, "C' positive at the upper bracket")
    for (i in 1:300) {
      mid <- (lo + hi) / 2
      if (eval_dterms(terms, mid) > 0) lo <- mid else hi <- mid
    }
    u <- (lo + hi) / 2
  }
  list(tmax = u + q$tlag, cmax = conc_vec(segs, u + q$tlag, FALSE), segs = segs)
}

# ---------------------------------------------------------------- extrema at steady state
# Breakpoints of the periodic profile are the dose time, the interval end, the start of the input
# (tlag modulo tau) and, for a finite-duration input, its end ((tlag + T) modulo tau); between two
# breakpoints the profile is analytic. Stationary points are found by sign changes of the derivative
# on a fine grid (128 bits would do; the sums are the 256-bit ones), refined by bisection.
fmod_mp <- function(x, tau) x - floor(x / tau) * tau
ss_extrema <- function(model, q, D, dur, tau, prep) {
  kd <- kind_of(model)
  bolus <- kd == "bolus"
  bp <- c(zero(), tau)
  if (!bolus && q$tlag > 0) bp <- c(bp, fmod_mp(q$tlag, tau))
  if (kd == "zero") bp <- c(bp, fmod_mp(q$tlag + mp(dur), tau))
  bp <- bp[order(as.numeric(bp))]
  bp <- bp[!duplicated(as.numeric(bp))]
  xs <- sort(unique(c(10^seq(-9, -1, length.out = 17), seq(0.1, 0.9, length.out = 17),
                      1 - 10^seq(-1, -9, length.out = 17))))
  cand <- c(bp, (bp[-length(bp)] + bp[-1]) / 2)    # the midpoints only reveal a plateau (a maximum attained everywhere)
  cm <- as.numeric(ss_eval(prep, bp[1]))
  dsum <- function(s) ss_eval(prep, s, deriv = TRUE)
  for (i in seq_len(length(bp) - 1L)) {
    b0 <- bp[i]; b1 <- bp[i + 1L]
    g <- b0 + mpfr(xs, PREC) * (b1 - b0)
    d <- vapply(seq_along(g), function(k) as.numeric(dsum(g[k])), 0)
    sc <- max(abs(d))
    if (sc < 1e-20 * cm / as.numeric(tau)) next      # a plateau: nothing to find
    for (k in seq_len(length(g) - 1L)) {
      if (d[k] == 0 || (d[k] > 0) != (d[k + 1L] > 0)) {
        if (abs(d[k]) < 1e-12 * sc && abs(d[k + 1L]) < 1e-12 * sc) next   # numerically flat
        lo <- g[k]; hi <- g[k + 1L]
        slo <- dsum(lo) > 0
        for (it in 1:80) {
          mid <- (lo + hi) / 2
          if ((dsum(mid) > 0) == slo) lo <- mid else hi <- mid
        }
        cand <- c(cand, (lo + hi) / 2)
      }
    }
  }
  vals <- do.call(c, lapply(seq_along(cand), function(k) ss_eval(prep, cand[k])))
  wrap <- function(x) if (bolus || x < tau) x else x - tau     # the interval is [0, tau) for a continuous profile
  pos <- lapply(seq_along(cand), function(k) wrap(cand[k]))
  imax <- which(vals == max(vals))[1]
  imin <- which(vals == min(vals))[1]
  near <- which(abs(vals - max(vals)) <= mp(2)^-60 * max(vals))
  tie <- length(unique(vapply(near, function(k) sprintf("%.12g", as.numeric(pos[[k]])), ""))) > 1L
  list(cmax = vals[imax], tmax = pos[[imax]], cmin = vals[imin], tmax_tied = tie)
}

# ---------------------------------------------------------------- ODE cross-checks (double)
# States y = (G depot, A1 central, A2 peripheral, AUC, R input rate); R is changed by events.
ode_params <- function(model, q) {
  list(V = num(q$vc), k10 = num(q$k10), k12 = num(q$k12), k21 = num(q$k21),
       ka = num(q$ka), tlag = num(q$tlag), kd = kind_of(model))
}
ode_rhs <- function(np) {
  force(np)
  function(t, y, parms) {
    dG <- if (np$kd == "first") -np$ka * y[1] else 0
    dA1 <- (if (np$kd == "first") np$ka * y[1] else 0) + y[5] - (np$k10 + np$k12) * y[2] + np$k21 * y[3]
    dA2 <- np$k12 * y[2] - np$k21 * y[3]
    list(c(dG, dA1, dA2, y[2] / np$V, 0))
  }
}
STATE <- c("G", "A1", "A2", "AUC", "R")
# events of a list of doses (data frame time, dose, dur)
dose_events <- function(np, recs) {
  ev <- data.frame(var = character(0), time = numeric(0), value = numeric(0), method = character(0))
  add <- function(var, time, value) ev <<- rbind(ev, data.frame(var = var, time = time, value = value, method = "add"))
  for (r in seq_len(nrow(recs))) {
    tj <- recs$time[r]; Dj <- recs$dose[r]
    if (np$kd == "bolus") {
      add("A1", tj, Dj)
    } else if (np$kd == "first") {
      add("G", tj + np$tlag, Dj)
    } else {
      R0 <- Dj / recs$dur[r]
      add("R", tj + np$tlag, R0)
      add("R", tj + np$tlag + recs$dur[r], -R0)
    }
  }
  # deSolve applies one event per variable and time: events at the same time are summed
  if (nrow(ev) > 0) {
    ev <- aggregate(value ~ var + time, data = ev, FUN = sum)
    ev$method <- "add"
  }
  ev[order(ev$time), , drop = FALSE]
}
ode_atol <- function(recs) 1e-14 * max(1, max(recs$dose))
# Concentration and AUC at `times` for a list of doses: rows at an event time hold the state before
# the events of that time, so a bolus at exactly that time is added back (it counts: MOD-GEN-02).
ode_schedule <- function(model, q, recs, times) {
  np <- ode_params(model, q)
  ev <- dose_events(np, recs)
  t0 <- min(c(times, ev$time)) - 1
  grid <- sort(unique(c(t0, times, ev$time)))
  sol <- ode(y = c(G = 0, A1 = 0, A2 = 0, AUC = 0, R = 0), times = grid, func = ode_rhs(np), parms = NULL,
             method = "lsoda", rtol = 1e-12, atol = ode_atol(recs), maxsteps = 1e6,
             events = if (nrow(ev) > 0) list(data = ev) else NULL)
  rows <- match(times, sol[, "time"])
  a1 <- sol[rows, "A1"]
  if (np$kd == "bolus") {
    for (i in seq_len(nrow(ev))) {
      hit <- which(times == ev$time[i])
      if (length(hit) > 0) a1[hit] <- a1[hit] + ev$value[i]
    }
  }
  list(conc = a1 / np$V, auc = sol[rows, "AUC"])
}

# The periodic solution at steady state. Time 0 is a nominal dose time; the lagged input of the
# train of doses at ..., -tau, 0, tau, ... enters at the phase tlag modulo tau, and a finite-duration
# input that started in the previous period is already running at time 0.
ode_steady <- function(model, q, D, dur, tau, svec) {
  np <- ode_params(model, q)
  kd <- np$kd
  act <- c(if (kd == "first") 1L, 2L, if (q$comp == 2L) 3L)
  Dn <- num(D); tn <- num(tau)
  e_on <- np$tlag - floor(np$tlag / tn) * tn
  ev0 <- list()      # events at time 0 (applied by hand): list of (idx, value)
  ev_in <- data.frame(var = character(0), time = numeric(0), value = numeric(0), method = character(0))
  r_start <- 0
  add_in <- function(var, time, value) {
    if (time == 0) ev0[[length(ev0) + 1L]] <<- list(var = var, value = value)
    else ev_in <<- rbind(ev_in, data.frame(var = var, time = time, value = value, method = "add"))
  }
  if (kd == "bolus") {
    ev0[[1]] <- list(var = "A1", value = Dn)
  } else if (kd == "first") {
    add_in("G", e_on, Dn)
  } else {
    Tn <- num(dur)
    R0 <- Dn / Tn
    if (Tn >= tn) {                       # continuous infusion: constant rate
      r_start <- R0
    } else {
      e_off <- (np$tlag + Tn) - floor((np$tlag + Tn) / tn) * tn
      if (e_off > e_on) {
        add_in("R", e_on, R0); add_in("R", e_off, -R0)
      } else {                            # the input wraps around the end of the period
        r_start <- R0
        add_in("R", e_off, -R0); add_in("R", e_on, R0)
      }
    }
  }
  ev_in <- ev_in[order(ev_in$time), , drop = FALSE]
  rhs <- ode_rhs(np)
  atol <- 1e-14 * max(1, Dn)
  run_period <- function(x_act, times_out) {
    y <- c(G = 0, A1 = 0, A2 = 0, AUC = 0, R = r_start)
    y[act] <- x_act
    for (e in ev0) y[e$var] <- y[e$var] + e$value
    grid <- sort(unique(c(0, times_out, ev_in$time)))
    ode(y = y, times = grid, func = rhs, parms = NULL, method = "lsoda", rtol = 1e-12, atol = atol,
        maxsteps = 1e6, events = if (nrow(ev_in) > 0) list(data = ev_in) else NULL)
  }
  # affine one-period map x -> M x + b on the active states (before the events of time 0)
  step <- function(x) {
    out <- run_period(x, tn)
    as.numeric(out[nrow(out), STATE[act] ])
  }
  b <- step(rep(0, length(act)))
  M <- sapply(seq_along(act), function(i) { e <- rep(0, length(act)); e[i] <- 1; step(e) - b })
  x_fp <- solve(diag(length(act)) - M, b)
  sol <- run_period(x_fp, svec)
  rows <- match(svec, sol[, "time"])
  list(conc = sol[rows, "A1"] / np$V, auc = sol[rows, "AUC"], x_fixed_point = x_fp)
}

# A long train of equal doses by events, read after the last dose (s since the last dose).
ode_train <- function(model, q, D, dur, tau, svec, m) {
  tn <- num(tau)
  recs <- data.frame(time = (0:m) * tn, dose = num(D), dur = if (is.null(dur)) NA_real_ else num(dur))
  last <- m * tn
  r <- ode_schedule(model, q, recs, c(last, last + svec))
  list(conc = r$conc[-1], auc = r$auc[-1] - r$auc[1])
}

rel_ok <- function(a, b, scale, tol = 1e-8) {
  all(abs(a - b) <= tol * pmax(abs(b), 1e-3 * scale))
}
rel_gap <- function(a, b, scale) max(abs(a - b) / pmax(abs(b), 1e-3 * scale))
sig2 <- function(x) as.numeric(sprintf("%.2g", x))

# ---------------------------------------------------------------- the tests of the ODE solver itself
local({
  # the rows at an event time hold the state before the event (relied upon in ode_schedule)
  rhs <- function(t, y, parms) list(c(-0.2 * y[1]))
  sol <- ode(y = c(A = 0), times = c(0, 1, 2, 3), func = rhs, parms = NULL, method = "lsoda",
             events = list(data = data.frame(var = "A", time = c(1, 2), value = c(10, 10), method = "add")))
  check(sol[2, "A"] == 0 && abs(sol[3, "A"] - 10 * exp(-0.2)) < 1e-4,
        "deSolve: a row at an event time is expected to hold the state before the event")
})

# ---------------------------------------------------------------- versions
versions <- list(
  R = R.version.string,
  Rmpfr = as.character(packageVersion("Rmpfr")),
  deSolve = as.character(packageVersion("deSolve")),
  gmp = as.character(packageVersion("gmp"))
)

# ---------------------------------------------------------------- time grids
# records: data frame (time, dose, dur), dur NA when the input has no duration.
sched_records_regular <- function(D, tau, n, dur) {
  data.frame(time = (0:(n - 1L)) * tau, dose = D, dur = dur %||% NA_real_)
}
sched_grid <- function(model, recs, tl, extra_pts) {
  kd <- kind_of(model)
  tj <- sort(recs$time)
  n <- length(tj)
  k <- max(1L, ceiling(n / 10))
  sel <- seq(1L, n, by = k)
  pts <- c(min(tj) - 1, tj, (tj + tl)[sel])
  if (kd == "zero") {
    d <- recs$dur[order(recs$time)]
    pts <- c(pts, (tj + tl + d)[sel], signif((tj + tl + d / 2)[sel], 6))
  }
  if (n >= 2L) {
    g <- c(diff(tj), diff(tj)[n - 1L])
    pts <- c(pts, signif(tj[sel] + 0.3 * g[sel], 6), signif(tj[sel] + 0.9 * g[sel], 6))
  } else {
    pts <- c(pts, tj + c(0.5, 2))
  }
  pts <- c(pts, tj[n] + c(1, 5, 20, 80, 300), extra_pts)
  sort(unique(pts))
}
ss_grid <- function(model, tau, tl, dur, extra_pts) {
  kd <- kind_of(model)
  tn <- as.numeric(tau)
  pts <- c(0, signif(tn * c(1e-6, 0.01, 0.05, 0.1, 0.25, 0.4, 0.5, 0.6, 0.75, 0.9, 0.99, 1 - 1e-6), 6), tn)
  if (kd != "bolus" && tl > 0) {
    e <- tl - floor(tl / tn) * tn
    pts <- c(pts, e, e + 1e-3 * tn, e - 1e-3 * tn)
  }
  if (kd == "zero") {
    e0 <- tl - floor(tl / tn) * tn
    e1 <- (tl + dur) - floor((tl + dur) / tn) * tn
    pts <- c(pts, e1, e1 + 1e-3 * tn, e1 - 1e-3 * tn, e0 + dur / 2)
    if (tl + dur <= tn) pts <- c(pts, tl + dur)
  }
  pts <- c(pts, extra_pts)
  pts <- pts[pts >= 0 & pts <= tn]
  sort(unique(pts))
}

# ---------------------------------------------------------------- a case
# regimen: list(kind = "steady_state", tau) | list(kind = "regular", tau, n) | list(kind = "schedule", records)
case_registry <- list()
add_case <- function(name, model, p, dose, extra, regimen, group, note, set = "clearance", grid = NULL,
                     convention = NULL) {
  case_registry[[length(case_registry) + 1L]] <<- list(
    name = name, model = model, set = set, p = p, dose = dose, extra = extra, regimen = regimen,
    group = group, note = note, grid = grid, convention = convention)
}

write_case <- function(cs) {
  model <- cs$model
  kd <- kind_of(model)
  bolus <- kd == "bolus"
  reg <- cs$regimen
  D <- cs$dose
  q <- build_q(model, cs$set, cs$p, D, cs$extra)
  tl <- num(q$tlag)
  dur <- cs$extra[["dur"]]
  name <- paste0(sub("\\.", "_", model), "_", cs$name)
  base <- paste0("model_md_", name)
  chk <- function(cond, ...) if (!isTRUE(cond)) stop(base, ": ", paste0(..., collapse = ""), call. = FALSE)
  scalars <- list()
  per_time <- list()
  printed <- NULL

  if (reg$kind %in% c("schedule", "regular")) {
    recs <- if (reg$kind == "regular") sched_records_regular(D, reg$tau, reg$n, dur) else reg$records
    times <- sort(unique(cs$grid %||% sched_grid(model, recs, tl, c(4, 12))))
    ev <- sched_eval(model, q, recs, mpfr(times, PREC))
    conc <- num(ev$conc); auc <- num(ev$auc)
    auc_inf <- sum(do.call(c, lapply(recs$dose, mp))) / q$cl
    scalars$auc_inf <- num(auc_inf)
    # the area to infinity is the sum of the single-dose areas, from the integrals of the terms
    ai <- zero()
    for (r in seq_len(nrow(recs))) ai <- ai + area_inf(segments_for(model, q, recs$dose[r], recs$dur[r]))
    chk(abs(ai - auc_inf) <= mp(2)^-150 * auc_inf, "area to infinity differs from sum(D) / CL")
    # ODE with dosing events
    od <- ode_schedule(model, q, recs, times)
    scale <- max(abs(conc)); sa <- max(abs(auc), 1e-300)
    chk(rel_ok(od$conc, conc, scale), "deSolve concentration: gap ", rel_gap(od$conc, conc, scale))
    chk(rel_ok(od$auc, auc, sa), "deSolve AUC: gap ", rel_gap(od$auc, auc, sa))
    cross <- list(ode_events = list(conc_max_rel_gap = sig2(rel_gap(od$conc, conc, scale)),
                                    auc_max_rel_gap = sig2(rel_gap(od$auc, auc, sa))))
    # MOD-MD-07, bolus of one compartment: C_n(s) = C_ss(s) (1 - exp(-n k tau)) after the last dose
    if (reg$kind == "regular" && model == "pk1.iv_bolus") {
      k <- q$lam[[1]]
      t_last <- (reg$n - 1L) * reg$tau
      sidx <- which(times >= t_last & times - t_last <= reg$tau)
      frac <- 1 - exp(-reg$n * k * reg$tau)
      for (i in sidx) {
        css <- ss_closed(model, q, D, dur, mp(reg$tau), mp(times[i]) - mp(t_last))
        chk(abs(ev$conc[i] - css * frac) <= mp(2)^-60 * abs(ev$conc[i]), "MOD-MD-07: fraction of steady state at t = ", times[i])
      }
    }
    per_time <- list(conc = conc, auc = auc)
    case_extra <- list(cross_checks = cross)
  } else {
    tau <- mp(reg$tau)
    tn <- reg$tau
    chk(is.null(dur) || dur <= tn, "tau must be at least the duration")
    segs <- segments_for(model, q, D, dur)
    s_num <- sort(unique(cs$grid %||% ss_grid(model, tau, tl, dur %||% 0, c(0.2, 1, 2, 3.5, 4))))
    sv <- mpfr(s_num, PREC)
    # number of terms: the tail of the geometric sum below 2^-TAILBITS of the smallest value
    N <- n_terms(segs, tau, TAILBITS + 25L)
    repeat {
      prep <- ss_prep(segs, bolus, tau, N)
      N <- prep$N
      vals <- do.call(c, lapply(seq_along(sv), function(i) ss_eval(prep, sv[i])))
      minv <- min(vals)
      chk(minv > 0, "a steady-state value is not positive")
      if (tail_bound(segs, N, tau) <= mp(2)^-TAILBITS * minv) break
      N <- as.integer(ceiling(N * 1.3)) + 5L
    }
    conc <- num(vals)
    areas <- do.call(c, lapply(seq_along(sv), function(i) ss_area_p(prep, sv[i])))
    # the factored sums against the plain vector sums, where the plain sums are affordable
    if (N <= 4000L) {
      for (i in unique(c(1L, ceiling(length(sv) / 2), length(sv)))) {
        chk(abs(ss_sum_direct(segs, bolus, tau, sv[i], N) - vals[i]) <= mp(2)^-150 * vals[i], "factored sum differs from the plain sum")
        chk(abs(ss_area_direct(segs, tau, sv[i], N) - areas[i]) <= mp(2)^-150 * max(areas[i], mp(1e-300)) + mp(2)^-300, "factored area differs from the plain sum")
      }
    }
    auc <- num(areas)
    # (1) the closed forms of the specification against the sum
    cf <- do.call(c, lapply(seq_along(sv), function(i) ss_closed(model, q, D, dur, tau, sv[i])))
    gap_cf <- max(abs(cf - vals) / abs(vals))
    chk(gap_cf <= AGREE, "closed forms of MOD-MD-03..06 differ from the sum: ", num(gap_cf))
    # (2) MOD-MD-09: the area over one interval is D / CL
    a_tau <- ss_area_p(prep, tau)
    d_over_cl <- mp(D) / q$cl
    chk(abs(a_tau - d_over_cl) <= mp(2)^-60 * d_over_cl, "area over one interval differs from D / CL")
    # (3) extrema and the derived quantities
    ex <- ss_extrema(model, q, D, dur, tau, prep)
    chk(tail_bound(segs, N, tau) <= mp(2)^-TAILBITS * ex$cmin, "tail above 2^-70 of the trough")
    sp <- single_peak(model, q, D, dur)
    area1 <- area_vec(segs, tau)
    cav <- d_over_cl / tau
    accum_cmax <- ex$cmax / sp$cmax
    accum_auc <- if (area1 == 0) NA else d_over_cl / area1      # a lag of at least tau: no area yet
    c1 <- conc_vec(segs, sv, bolus)
    accum_c <- rep(NA_real_, length(s_num))
    for (i in seq_along(s_num)) if (c1[i] != 0) accum_c[i] <- num(vals[i] / c1[i])
    # (4) the peak and trough by the closed forms of MOD-MD-10
    spec_tmax <- NULL
    if (bolus) {
      spec_cmax <- ss_closed(model, q, D, dur, tau, zero())
      spec_cmin <- ss_closed(model, q, D, dur, tau, tau)
      spec_tmax <- zero()
    } else {
      e_in <- if (tl > 0) fmod_mp(q$tlag, tau) else zero()
      spec_cmin <- ss_closed(model, q, D, dur, tau, e_in)
      if (kd == "zero") {
        spec_tmax <- fmod_mp(q$tlag + mp(dur), tau)
        spec_cmax <- ss_closed(model, q, D, dur, tau, spec_tmax)
      } else if (q$comp == 1L) {
        k <- q$lam[[1]]; ka <- q$ka
        u <- if (abs(ka - k) < TINY) 1 / k - tau / (exp(k * tau) - 1)
             else log(ka * (1 - exp(-k * tau)) / (k * (1 - exp(-ka * tau)))) / (ka - k)
        spec_tmax <- fmod_mp(u + q$tlag, tau)
        spec_cmax <- ss_closed(model, q, D, dur, tau, spec_tmax)
      } else {
        # two compartments, no closed form: the peak lies between the one-compartment ones
        xs <- lapply(q$lam, function(l) {
          ka <- q$ka
          if (abs(ka - l) < TINY) 1 / l - tau / (exp(l * tau) - 1)
          else log(ka * (1 - exp(-l * tau)) / (l * (1 - exp(-ka * tau)))) / (ka - l)
        })
        lo <- min(xs[[1]], xs[[2]]); hi <- max(xs[[1]], xs[[2]])
        u <- fmod_mp(ex$tmax - q$tlag, tau)
        chk(u >= lo * (1 - mp(2)^-60) && u <= hi * (1 + mp(2)^-60), "Tmax,ss outside its bracket")
        spec_cmax <- ex$cmax
      }
    }
    chk(abs(spec_cmax - ex$cmax) <= AGREE * ex$cmax, "Cmax,ss differs from MOD-MD-10")
    chk(abs(spec_cmin - ex$cmin) <= AGREE * ex$cmin, "Cmin,ss differs from MOD-MD-10")
    if (!is.null(spec_tmax) && !ex$tmax_tied) {
      chk(abs(spec_tmax - ex$tmax) <= mp(2)^-50 * max(abs(spec_tmax), mp(1e-30)) + mp(2)^-100,
          "Tmax,ss differs from MOD-MD-10: ", num(spec_tmax), " against ", num(ex$tmax))
    }
    # (5) S-36's printed form for the interval before the end of the lag
    if (!bolus && tl > 0) {
      sel <- which(s_num <= tl)
      pr <- lapply(sel, function(i) s36_printed(model, q, D, dur, tau, sv[i]))
      ok <- !vapply(pr, is.null, TRUE)
      sel <- sel[ok]; pr <- pr[ok]
      if (length(sel) > 0) {
        prn <- vapply(pr, num, 0)
        bad <- abs(prn - conc[sel]) / abs(conc[sel]) > 1e-9
        printed <- list(
          branch = "S-36 p. 10 eq. 1.16 (first-order input) and p. 14 eq. 1.26 (zero-order input), branch s <= tlag: the earlier dose in its decay phase at the elapsed time s + tau - tlag",
          differs_from_oracle = any(bad),
          times_that_differ = arr(s_num[sel][bad]),
          printed_values = arr(prn[bad]),
          oracle_values = arr(conc[sel][bad]))
      }
    }
    # (6) ODE: periodic solution, and a long train of doses
    scale <- max(abs(conc)); sa <- max(abs(auc))
    od <- ode_steady(model, q, mp(D), dur, tau, s_num)
    chk(rel_ok(od$conc, conc, scale), "deSolve periodic solution, concentration: gap ", rel_gap(od$conc, conc, scale))
    chk(rel_ok(od$auc, auc, sa), "deSolve periodic solution, AUC: gap ", rel_gap(od$auc, auc, sa))
    cross <- list(ode_periodic_solution = list(conc_max_rel_gap = sig2(rel_gap(od$conc, conc, scale)),
                                               auc_max_rel_gap = sig2(rel_gap(od$auc, auc, sa))))
    m <- ceiling(33 / (lam_min_of(segs) * tn)) + 3
    if (m <= 3000) {
      tr <- ode_train(model, q, mp(D), dur, tau, s_num, m)
      chk(rel_ok(tr$conc, conc, scale), "deSolve long train, concentration: gap ", rel_gap(tr$conc, conc, scale))
      chk(rel_ok(tr$auc, auc, sa), "deSolve long train, AUC: gap ", rel_gap(tr$auc, auc, sa))
      cross$ode_long_train <- list(doses = as.integer(m + 1), conc_max_rel_gap = sig2(rel_gap(tr$conc, conc, scale)),
                                   auc_max_rel_gap = sig2(rel_gap(tr$auc, auc, sa)))
    } else {
      cross$ode_long_train <- list(skipped = paste0(as.integer(m + 1), " doses would be needed (more than 3000)"))
    }
    times <- s_num
    per_time <- list(conc = conc, auc = auc, accum_c = accum_c)
    scalars$cmax_ss <- num(ex$cmax)
    scalars$tmax_ss <- if (ex$tmax_tied) NA_real_ else num(ex$tmax)
    scalars$cmin_ss <- num(ex$cmin)
    scalars$cav_ss <- num(cav)
    scalars$auc_tau_ss <- num(d_over_cl)
    scalars$accum_cmax <- num(accum_cmax)
    scalars$accum_auc <- num(accum_auc)
    case_extra <- list(
      steady_state = list(terms_in_sum = as.integer(N + 1L),
                          tail_bound_relative = sig2(num(tail_bound(segs, N, tau) / minv)),
                          closed_form_max_rel_gap = sig2(num(gap_cf)),
                          tmax_ss_is_not_unique = ex$tmax_tied),
      cross_checks = cross)
    if (!is.null(printed)) case_extra$s36_printed_form <- printed
  }

  # --- tables
  tk <- paste0("t=", sprintf("%.17g", times))
  rows <- list()
  for (i in seq_along(times)) {
    for (qn in names(per_time)) rows[[length(rows) + 1L]] <- c(tk[i], qn, num17(per_time[[qn]][i]))
  }
  for (sn in names(scalars)) rows[[length(rows) + 1L]] <- c("scalar", sn, num17(scalars[[sn]]))
  rows <- do.call(rbind, rows)
  write_lines_lf(c("subject,parameter,value", paste(rows[, 1], rows[, 2], rows[, 3], sep = ",")),
                 file.path(exp_dir, paste0(base, ".csv")))

  reg_json <- switch(reg$kind,
    steady_state = list(kind = "steady_state", tau = reg$tau),
    regular = list(kind = "regular", tau = reg$tau, n_doses = as.integer(reg$n)),
    schedule = list(kind = "schedule", records = lapply(seq_len(nrow(reg$records)), function(i) {
      r <- list(time = reg$records$time[i], dose = reg$records$dose[i])
      if (!is.na(reg$records$dur[i])) r$dur <- reg$records$dur[i]
      r
    })))
  tex <- cs$set == "clearance" && {
    if (n_comp(model) == 1L) {
      is.null(q$ka) || abs(q$ka - q$lam[[1]]) > 5e-2 * q$ka
    } else {
      al <- num(q$lam[[1]]); be <- num(q$lam[[2]])
      ok <- (al - be) / al > 1e-2
      if (!is.null(q$ka)) ok <- ok && abs(num(q$ka) - al) > 5e-2 * num(q$ka) && abs(num(q$ka) - be) > 5e-2 * num(q$ka)
      ok
    }
  }
  is_ss <- reg$kind == "steady_state"
  quantities <- list(
    conc = if (is_ss) "concentration of the central compartment at the time s since the last dose, 0 <= s <= tau, at steady state: the sum over j >= 0 of the single-dose function at s + j*tau (a bolus at s = 0 includes the dose; at s = tau it is the value just before the next one)"
           else "concentration of the central compartment at the time t of the origin shared by the dose times: the sum of the single-dose functions of every dose (0 before the first dose; a bolus exactly at its dose time counts)",
    auc = if (is_ss) "area under the steady-state profile from s = 0 to s (D/CL at s = tau)"
          else "sum of the single-dose areas, each from its own dose time (not from its lag), to the time")
  if (is_ss) {
    quantities$accum_c <- "R_C(s) = C_ss(s) / C_1(s), the ratio to the single-dose concentration at the same time since the dose (MOD-MD-08); not available where C_1(s) = 0 (s up to the lag, or s = 0 for an input that starts at 0)"
    quantities$scalar <- "cmax_ss, tmax_ss (time since the dose in [0, tau); not available when the maximum is attained at every s: a continuous infusion, tau = T), cmin_ss, cav_ss = D/(CL*tau), auc_tau_ss = D/CL, accum_cmax = cmax_ss / (single-dose Cmax), accum_auc = (D/CL) / AUC(0, tau) of one dose (not available when that area is 0, a lag of at least tau) (MOD-MD-08 to 10)"
  } else {
    quantities$scalar <- "auc_inf = sum of the doses / CL (the sum of the single-dose areas to infinity, MOD-MD-01)"
  }
  meta <- list(
    schema = 1L, kind = "model_md", case = base,
    generated_by = "oracle/scripts/models_md.R",
    model = model,
    group = cs$group,
    parameterisation = cs$set,
    dose = D,
    parameters = c(cs$p, cs$extra[setdiff(names(cs$extra), "")]),
    regimen = reg_json,
    times = arr(times),
    units = list(dose = "mg", time = "h", concentration = "mg/L"),
    note = cs$note,
    textbook_double_ok = isTRUE(tex),
    quantities = quantities,
    method = list(
      closed_form = "plain sum of the single-dose closed forms of the model (explicit sums of exponentials, MOD-IVB-01, MOD-IVI-01, MOD-AB1-01, MOD-AB0-01, MOD-2C-07..12, the lag as a delay), 256-bit arithmetic (Rmpfr), rounded once to double; areas are the exact integrals of the terms; steady state is the sum over j = 0..N with the tail bounded below 2^-70 of the smallest value; the geometric-series forms of MOD-MD-03..06 and 10 give no value and are checked against the sums in the script (agreement 2^-60)",
      cross_checks = list("adaptive ODE solver (deSolve lsoda, rtol 1e-12) with the doses as events and a state for the AUC",
                          "steady state: periodic solution of the linear system (fixed point of the one-period map) and, up to 3000 doses, a long train of dosing events"),
      cross_check_tolerance = 1e-8
    ),
    results_of_checks = case_extra,
    convention = cs$convention,
    versions = versions,
    n_values = nrow(rows),
    n_times = length(times)
  )
  if (is.null(meta$convention)) meta$convention <- NULL
  write_lines_lf(jenc(meta), file.path(exp_dir, paste0(base, ".options.json")))
  cat(sprintf("%-62s %3d times %5d values\n", base, length(times), nrow(rows)))
  list(name = name, base = base, group = cs$group, kind = reg$kind, n = nrow(rows), model = model)
}

# ---------------------------------------------------------------- parameter points
P1 <- list(v = 10, cl = 2)                       # k = 0.2
P2 <- list(cl = 2, vc = 10, q = 4, vp = 8)       # alpha = 1, beta = 0.1 (worked example P2)
FAM_P <- list(pk1 = P1, pk2 = P2)
TAU_DEF <- c(pk1 = 6, pk2 = 12)
KA_DEF <- c(pk1 = 1, pk2 = 2)
IDS <- c("iv_bolus", "iv_infusion", "oral_1", "oral_1_lag", "oral_0", "oral_0_lag")
mid <- function(fam, id) paste0(fam, ".", id)
tg <- function(x) gsub("\\.", "p", format(x, scientific = FALSE, digits = 10, trim = TRUE))

# alpha = 1, beta = 1e-3, weight 0.3, Vc = 20, rounded to 10 significant digits (as in T-032)
AB <- local({
  V <- mp(20); al <- mp(1); be <- mp(1e-3); wa <- mp(0.3); wb <- 1 - wa
  k21 <- al * wb + be * wa; k10 <- al * be / k21; k12 <- al + be - k10 - k21
  list(cl = signif(num(k10 * V), 10), vc = 20, q = signif(num(k12 * V), 10), vp = signif(num(k12 * V / k21), 10))
})
# the macro set of the base point, for ka = alpha and ka = beta exactly
MACRO <- local({
  qb <- build_q("pk2.iv_bolus", "clearance", P2, 100, list())
  list(a = num(100 * qb$w[[1]] / qb$vc), b = num(100 * qb$w[[2]] / qb$vc),
       alpha = num(qb$lam[[1]]), beta = num(qb$lam[[2]]))
})

std_extra <- function(fam, id) {
  switch(id,
    iv_bolus = list(),
    iv_infusion = , oral_0 = list(dur = 2),
    oral_0_lag = list(dur = 2, tlag = 0.5),
    oral_1 = list(ka = KA_DEF[[fam]]),
    oral_1_lag = list(ka = KA_DEF[[fam]], tlag = 0.5))
}

ss <- function(fam, id, name, extra, tau, group = "steady_state", note = "", p = FAM_P[[fam]], set = "clearance",
               dose = 100, convention = NULL) {
  add_case(name, mid(fam, id), p, dose, extra, list(kind = "steady_state", tau = tau), group, note, set,
           convention = convention)
}

# ---------------------------------------------------------------- superposition: schedules and regular regimens
sched_recs <- function(n, zero) {
  r <- if (n == 1L) {
    data.frame(time = 2.5, dose = 80, dur = 3)
  } else if (n == 3L) {
    if (zero) data.frame(time = c(0, 2, 9), dose = c(100, 100, 50), dur = c(3, 3, 2))
    else data.frame(time = c(0, 4, 10), dose = c(100, 50, 100), dur = NA_real_)
  } else if (n == 10L) {
    # unsorted, a zero dose, overlaps
    d <- data.frame(time = c(0, 1.5, 4, 5, 9.25, 12, 14.5, 20, 21, 27.5),
                    dose = c(100, 50, 150, 100, 75, 100, 200, 0, 100, 25),
                    dur = c(2, 1, 3, 2, 4, 1, 2, 2, 5, 1))
    d[c(3, 1, 7, 2, 10, 5, 9, 4, 8, 6), ]
  } else {
    j <- 1:40
    d <- data.frame(time = 3 * (j - 1) + ((7 * j) %% 5) / 4, dose = 100 * (1 + ((3 * j) %% 4) / 4),
                    dur = 1 + ((5 * j) %% 7) / 2)
    d$dose[17] <- 0
    d[40:1, ]
  }
  if (!zero) r$dur <- NA_real_
  rownames(r) <- NULL
  r
}

for (fam in c("pk1", "pk2")) {
  for (id in IDS) {
    model <- mid(fam, id)
    kd <- kind_of(model)
    ex <- std_extra(fam, id)
    ex_sched <- ex[setdiff(names(ex), "dur")]      # the duration belongs to each record
    for (n in c(1L, 3L, 10L, 40L)) {
      add_case(paste0("sched_n", n), model, FAM_P[[fam]], 100, ex_sched,
               list(kind = "schedule", records = sched_recs(n, kd == "zero")), "superposition",
               paste0("schedule of ", n, " dose", if (n > 1) "s" else "", ", irregular times and varying amounts",
                      if (n == 10L) ", unsorted records, a zero dose" else "",
                      if (n == 40L) ", records in reverse order, a zero dose" else "",
                      if (kd == "zero" && n >= 3L) ", infusions that overlap" else ""))
      add_case(paste0("regular_n", n), model, FAM_P[[fam]], 100, ex,
               list(kind = "regular", tau = TAU_DEF[[fam]], n = n), "superposition",
               paste0(n, " equal doses at interval ", TAU_DEF[[fam]], " (MOD-MD-07)"))
    }
  }
}

# ---------------------------------------------------------------- steady state, one compartment and two
for (fam in c("pk1", "pk2")) {
  one <- fam == "pk1"
  td <- TAU_DEF[[fam]]
  heavy <- if (one) 0.005 else 0.01            # k * tau = 1e-3 (slowest exponent 0.2 and 0.1)
  far <- if (one) 250 else 500                 # k * tau = 50 (slowest exponent 0.2 and 0.1)
  alpha1 <- if (one) 0.2 else 1                # the rate ka is compared with: k, alpha

  # IV bolus
  for (tau in c(heavy, if (one) 1 else 2, td, 4 * td, far)) {
    ss(fam, "iv_bolus", paste0("ss_tau_", tg(tau)), list(), tau,
       note = paste0("regular IV boluses, interval ", tau, " (slowest exponent times interval: ",
                     signif(tau * (if (one) 0.2 else 0.1), 3), ")"))
  }
  if (!one) ss(fam, "iv_bolus", "ss_ab_ratio_1e3_tau_24", list(), 24, p = AB, dose = 400,
               note = "alpha = 1, beta = 1e-3 (ratio 1e3), interval 24")

  # IV infusion and zero-order absorption, T = 2
  for (id in c("iv_infusion", "oral_0")) {
    for (tau in c(2, 3, td, 20, far)) {
      ss(fam, id, paste0("ss_tau_", tg(tau)), list(dur = 2), tau,
         note = paste0("regular ", if (id == "iv_infusion") "infusions" else "zero-order inputs", " of duration 2, interval ", tau,
                       if (tau == 2) " (= T: the plateau R0/CL)" else ""))
    }
    ss(fam, id, paste0("ss_tau_", tg(heavy), "_dur_", tg(0.8 * heavy)), list(dur = 0.8 * heavy), heavy,
       note = "slowest exponent times interval 1e-3, duration 0.8 of the interval")
  }
  ss(fam, "iv_infusion", if (one) "ss_tau_6_dur_6" else "ss_ab_ratio_1e3_tau_100_dur_50", list(dur = if (one) 6 else 50), if (one) 6 else 100,
     p = if (one) P1 else AB, dose = if (one) 100 else 400,
     note = if (one) "T = tau = 6 (plateau)" else "alpha = 1, beta = 1e-3, T = 50, interval 100")

  # first-order absorption, ka relative to the exponents
  kas <- if (one) list(c("1", 1), c("2", 2), c("100", 100), c("k", 0.2), c("k_1p001", 0.2 * (1 + 1e-3)),
                       c("k_1p000001", 0.2 * (1 + 1e-6)), c("0p1", 0.1))
         else list(c("2", 2), c("500", 500), c("0p4", 0.4), c("0p05", 0.05), c("alpha_1p001", 1 * (1 + 1e-3)),
                   c("alpha_1p000001", 1 * (1 + 1e-6)))
  for (k in kas) {
    ss(fam, "oral_1", paste0("ss_ka_", k[1], "_tau_", tg(td)), list(ka = as.numeric(k[2])), td,
       note = paste0("first-order absorption, ka = ", k[2], ", interval ", td))
  }
  if (!one) {
    ss(fam, "oral_1", "ss_ka_eq_alpha_macro", list(ka = MACRO$alpha), td, p = MACRO, set = "macro",
       note = "ka = alpha exactly (the confluent limit), macro set")
    ss(fam, "oral_1", "ss_ka_eq_beta_macro", list(ka = MACRO$beta), td, p = MACRO, set = "macro",
       note = "ka = beta exactly (the confluent limit), macro set")
  }
  ka0 <- KA_DEF[[fam]]
  for (tau in c(heavy, if (one) 1 else 4, 4 * td, far)) {
    ss(fam, "oral_1", paste0("ss_ka_", tg(ka0), "_tau_", tg(tau)), list(ka = ka0), tau,
       note = paste0("first-order absorption, ka = ", ka0, ", interval ", tau))
  }

  # first-order absorption with lag
  tl_cases <- if (one) list(c("0p5", 0.5, 6, 1), c("3", 3, 6, 1), c("0p5", 0.5, 1, 1), c("8p5", 8.5, 24, 1)) else
                       list(c("0p5", 0.5, 12, 2), c("6", 6, 12, 2), c("0p5", 0.5, 1, 2), c("20", 20, 48, 2))
  for (k in tl_cases) {
    ss(fam, "oral_1_lag", paste0("ss_tlag_", k[1], "_tau_", tg(as.numeric(k[3]))), list(ka = ka0, tlag = as.numeric(k[2])),
       as.numeric(k[3]), note = paste0("first-order absorption with lag ", k[2], ", interval ", k[3]))
  }
  if (one) {
    for (k in list(c("k", 0.2), c("k_1p000001", 0.2 * (1 + 1e-6)), c("100", 100), c("0p1", 0.1))) {
      ss(fam, "oral_1_lag", paste0("ss_ka_", k[1], "_tlag_0p5_tau_6"), list(ka = as.numeric(k[2]), tlag = 0.5), 6,
         note = paste0("first-order absorption with lag 0.5, ka = ", k[2]))
    }
  } else {
    ss(fam, "oral_1_lag", "ss_ka_eq_alpha_macro_tlag_0p5", list(ka = MACRO$alpha, tlag = 0.5), 12, p = MACRO, set = "macro",
       note = "ka = alpha exactly with lag 0.5, macro set")
    ss(fam, "oral_1_lag", "ss_ka_500_tlag_0p5_tau_12", list(ka = 500, tlag = 0.5), 12, note = "ka = 500 alpha with lag 0.5")
  }

  # zero-order absorption with lag, T = 2
  zl <- if (one) list(c(6, 1), c(6, 4), c(6, 0.5), c(6, 5.5), c(6, 6), c(20, 3), c(10, 12))
        else list(c(12, 1), c(12, 10), c(12, 0.5), c(12, 11), c(12, 12), c(48, 20), c(20, 30))
  for (k in zl) {
    ss(fam, "oral_0_lag", paste0("ss_tau_", tg(k[1]), "_tlag_", tg(k[2])), list(dur = 2, tlag = k[2]), k[1],
       note = paste0("zero-order absorption (T = 2) with lag ", k[2], ", interval ", k[1],
                     if (k[1] >= k[2] + 2) " (interval at least tlag + T)" else " (interval below tlag + T)"))
  }
}

# ---------------------------------------------------------------- conventions of OM-17 (e) and (f)
CONV_E <- "OM-17 (e): the steady-state lag forms of S-36 print a branch for s <= tlag that is valid only when the earlier dose is already in its decay phase, tau >= tlag (first-order input) or tau >= tlag + T (zero-order input); MOD-MD-05 reads the profile at the phase (s - tlag) modulo tau and has no such condition. The expected values follow MOD-MD-05; the printed branch is evaluated in results_of_checks.s36_printed_form and listed where it differs"
CONV_F <- "OM-17 (f): S-36 asks for strict inequalities (spacing and interval longer than the duration, p. 4); MOD-MD-01 has no spacing condition and the steady-state rules allow tau = T (a continuous infusion, plateau R0/CL), refusing only tau < T"
for (fam in c("pk1", "pk2")) {
  td <- TAU_DEF[[fam]]
  ka0 <- KA_DEF[[fam]]
  # (e) zero-order input with lag
  for (k in list(c(2.5, 1), c(3, 2), c(3, 1), c(3.000001, 1), c(2.999999, 1), c(2, 1), c(2, 5), c(6, 1))) {
    ss(fam, "oral_0_lag", paste0("conv_tau_", tg(k[1]), "_tlag_", tg(k[2])), list(dur = 2, tlag = k[2]), k[1],
       group = "convention", convention = list(id = "OM-17 (e)", text = CONV_E),
       note = paste0("zero-order absorption T = 2, tau = ", k[1], ", tlag = ", k[2], "; tlag + T = ", k[2] + 2,
                     if (k[1] < k[2] + 2) ": below it, the printed S-36 branch fails" else ": at or above it, the printed branch is exact"))
  }
  # (e) first-order input with lag
  for (k in list(c("tlag_eq_tau", td), c("tlag_below_tau_1e6", td * (1 - 1e-6)), c("tlag_above_tau_1e6", td * (1 + 1e-6)),
                 c("tlag_2tau", 2 * td), c("tlag_gt_tau", 1.4166666666666667 * td))) {
    ss(fam, "oral_1_lag", paste0("conv_", k[1]), list(ka = ka0, tlag = as.numeric(k[2])), td,
       group = "convention", convention = list(id = "OM-17 (e)", text = CONV_E),
       note = paste0("first-order absorption, ka = ", ka0, ", tau = ", td, ", tlag = ", k[2], if (as.numeric(k[2]) > td) ": above tau, the printed S-36 branch needs a negative elapsed time" else ""))
  }
  # (f) tau = T and tau just above T (infusion, zero-order, zero-order with lag)
  for (id in c("iv_infusion", "oral_0", "oral_0_lag")) {
    lagx <- if (id == "oral_0_lag") list(tlag = 0.5) else list()
    ss(fam, id, "conv_tau_eq_T", c(list(dur = 2), lagx), 2, group = "convention", convention = list(id = "OM-17 (f)", text = CONV_F),
       note = "tau = T = 2: a continuous input, the profile is the plateau R0/CL (the non-strict inequality of MOD-MD-12)")
    ss(fam, id, "conv_tau_T_plus_2e6", c(list(dur = 2), lagx), 2.000002, group = "convention", convention = list(id = "OM-17 (f)", text = CONV_F),
       note = "tau = T + 2e-6: almost continuous input")
    ss(fam, id, "conv_tau_T_plus_2e9", c(list(dur = 2), lagx), 2.000000002, group = "convention", convention = list(id = "OM-17 (f)", text = CONV_F),
       note = "tau = T + 2e-9: the profile is within 1e-9 of the plateau, no loss of digits expected")
    add_case("conv_regular_overlap_n5", mid(fam, id), FAM_P[[fam]], 100, c(list(dur = 2), lagx),
             list(kind = "regular", tau = 1, n = 5L), "convention",
             "5 equal doses at interval 1 with T = 2: overlapping inputs, no spacing condition (MOD-MD-01)",
             convention = list(id = "OM-17 (f)", text = CONV_F))
    add_case("conv_sched_gap_eq_T", mid(fam, id), FAM_P[[fam]], 100, lagx,
             list(kind = "schedule", records = data.frame(time = c(0, 2, 4, 6), dose = 100, dur = 2)), "convention",
             "four infusions of duration 2 at 0, 2, 4, 6: each starts exactly when the previous one ends",
             convention = list(id = "OM-17 (f)", text = CONV_F))
    add_case("conv_sched_gap_lt_T", mid(fam, id), FAM_P[[fam]], 100, lagx,
             list(kind = "schedule", records = data.frame(time = c(0, 1, 2, 3), dose = 100, dur = 2)), "convention",
             "four infusions of duration 2 at 0, 1, 2, 3: each starts before the previous one ends",
             convention = list(id = "OM-17 (f)", text = CONV_F))
  }
}

# ---------------------------------------------------------------- the error cases (MOD-MD-12)
errors <- list()
err <- function(id, group, spec, model, dose, params, words, reason, times = list(1)) {
  errors[[length(errors) + 1L]] <<- list(
    id = id, group = group, spec = spec, model = model, dose = dose, parameters = params, times = times,
    message_contains = lapply(words, arr), reason = reason)
}
with_p <- function(p, ...) { new <- list(...); c(p[setdiff(names(p), names(new))], new) }
SS <- "MOD-MD-12, MOD-MD-02"
for (bad in list(c("zero", 0), c("negative", -6), c("nan", "NaN"), c("infinite", "Infinity"))) {
  v <- if (bad[2] %in% c("NaN", "Infinity")) bad[2] else as.numeric(bad[2])
  err(paste0("tau_", bad[1], "_pk1_iv_bolus"), "tau", SS, "pk1.iv_bolus", 100, c(P1, list(tau = v)), list("tau"),
      "`tau` must be finite and > 0")
}
err("tau_zero_pk1_oral_1", "tau", SS, "pk1.oral_1", 100, c(P1, list(ka = 1, tau = 0)), list("tau"), "`tau` must be > 0")
err("tau_zero_pk1_oral_0_lag", "tau", SS, "pk1.oral_0_lag", 100, c(P1, list(dur = 2, tlag = 0.5, tau = 0)), list("tau"), "`tau` must be > 0")
err("tau_zero_pk2_iv_bolus", "tau", SS, "pk2.iv_bolus", 100, c(P2, list(tau = 0)), list("tau"), "`tau` must be > 0")
err("tau_negative_pk2_oral_1_lag", "tau", SS, "pk2.oral_1_lag", 100, c(P2, list(ka = 2, tlag = 0.5, tau = -12)), list("tau"), "`tau` must be > 0")
err("tau_nan_pk2_iv_infusion", "tau", SS, "pk2.iv_infusion", 100, c(P2, list(dur = 2, tau = "NaN")), list("tau"), "`tau` must be finite")
BELOW <- "an infusion or zero-order absorption longer than the interval overlaps the earlier ones: the steady-state rules refuse it and the message says to use the schedule form (MOD-MD-12)"
for (e in list(list("pk1.iv_infusion", P1, list(dur = 2), 1.9), list("pk1.iv_infusion", P1, list(dur = 2), 2 * (1 - 1e-9)),
               list("pk1.oral_0", P1, list(dur = 2), 1), list("pk1.oral_0_lag", P1, list(dur = 2, tlag = 1), 1.9),
               list("pk2.iv_infusion", P2, list(dur = 2), 1.5), list("pk2.oral_0", P2, list(dur = 2), 0.5),
               list("pk2.oral_0_lag", P2, list(dur = 2, tlag = 1), 1.9))) {
  err(paste0("tau_below_dur_", sub("\\.", "_", e[[1]]), "_", tg(e[[4]])), "tau_below_dur", "MOD-MD-12, MOD-MD-02 (f)", e[[1]], 100,
      c(e[[2]], e[[3]], list(tau = e[[4]])), list("tau", c("dur", "duration"), c("schedule", "overlap")), BELOW)
}
for (e in list(list("pk1.iv_bolus", P1, list(), 6, -0.5), list("pk1.iv_bolus", P1, list(), 6, 6.5), list("pk1.oral_1", P1, list(ka = 1), 6, -1e-9),
               list("pk1.iv_infusion", P1, list(dur = 2), 6, 7), list("pk2.oral_1", P2, list(ka = 2), 12, 12.5),
               list("pk2.iv_bolus", P2, list(), 12, 24), list("pk1.oral_1_lag", P1, list(ka = 1, tlag = 0.5), 6, 6.0001))) {
  err(paste0("time_outside_", sub("\\.", "_", e[[1]]), "_", tg(e[[5]])), "times", "MOD-MD-12, MOD-MD-02 (f)", e[[1]], 100,
      c(e[[2]], e[[3]], list(tau = e[[4]])), list("time", c("tau", "interval")),
      "a time outside [0, tau] in a steady-state request is a readable error", times = list(1, e[[5]]))
}
err("time_nan_steady", "times", "MOD-MD-12, MOD-GEN-04", "pk1.iv_bolus", 100, c(P1, list(tau = 6)), list("time", c("finite", "nan")),
    "a NaN time is refused", times = list(1, "NaN"))
for (bad in list(c("zero", 0), c("negative", -1), c("nan", "NaN"))) {
  v <- if (bad[2] == "NaN") bad[2] else as.numeric(bad[2])
  err(paste0("n_doses_", bad[1], "_pk1_iv_bolus"), "n_doses", "MOD-MD-12", "pk1.iv_bolus", 100, c(P1, list(tau = 6, n_doses = v)),
      list(c("n_doses", "doses")), "`n_doses` must be an integer >= 1")
}
err("n_doses_zero_pk2_oral_1", "n_doses", "MOD-MD-12", "pk2.oral_1", 100, c(P2, list(ka = 2, tau = 12, n_doses = 0)), list(c("n_doses", "doses")),
    "`n_doses` must be an integer >= 1")
err("n_doses_with_tau_zero", "n_doses", "MOD-MD-12", "pk1.iv_bolus", 100, c(P1, list(tau = 0, n_doses = 3)), list("tau"), "`tau` must be > 0 for a regular regimen too")
rec <- function(i, time, dose, dur = NULL) {
  r <- list(time, dose); names(r) <- c(paste0("dose_time[", i, "]"), paste0("dose_amount[", i, "]"))
  if (!is.null(dur)) r[[paste0("dose_dur[", i, "]")]] <- dur
  r
}
SCH <- "MOD-MD-12, MOD-MD-01"
base_sched <- c(rec(0, 0, 100), rec(1, 6, 100))
err("schedule_time_nan", "schedule", SCH, "pk1.iv_bolus", 100, c(P1, rec(0, 0, 100), rec(1, "NaN", 100)), list("time"), "a non-finite dose time", times = list(1, 8))
err("schedule_time_infinite", "schedule", SCH, "pk1.oral_1", 100, c(P1, list(ka = 1), rec(0, "Infinity", 100)), list("time"), "a non-finite dose time")
err("schedule_dose_negative", "schedule", SCH, "pk1.iv_bolus", 100, c(P1, rec(0, 0, 100), rec(1, 6, -5)), list("dose"), "a negative dose amount")
err("schedule_dose_nan", "schedule", SCH, "pk2.iv_bolus", 100, c(P2, rec(0, 0, "NaN")), list("dose"), "a non-finite dose amount")
err("schedule_dur_zero", "schedule", SCH, "pk1.iv_infusion", 100, c(P1, rec(0, 0, 100, 0)), list("dur"), "a non-positive duration for a finite-duration input")
err("schedule_dur_negative", "schedule", SCH, "pk1.oral_0", 100, c(P1, rec(0, 0, 100, 2), rec(1, 6, 100, -1)), list("dur"), "a negative duration")
err("schedule_dur_nan", "schedule", SCH, "pk2.oral_0_lag", 100, c(P2, list(tlag = 0.5), rec(0, 0, 100, "NaN")), list("dur"), "a non-finite duration")
err("schedule_dur_missing", "schedule", SCH, "pk1.iv_infusion", 100, c(P1, rec(0, 0, 100)), list("dur"), "a record of a finite-duration input without a duration")
err("schedule_dur_missing_pk2", "schedule", SCH, "pk2.iv_infusion", 100, c(P2, rec(0, 0, 100), rec(1, 4, 100, 2)), list("dur"), "the first record has no duration")
err("tlag_negative_steady", "lag", "MOD-MD-12, MOD-GEN-04", "pk1.oral_1_lag", 100, c(P1, list(ka = 1, tlag = -0.5, tau = 6)), list("tlag"), "`tlag` must be >= 0 (a lag longer than tau is allowed)")
err("tlag_nan_steady", "lag", "MOD-MD-12, MOD-GEN-04", "pk2.oral_0_lag", 100, c(P2, list(dur = 2, tlag = "NaN", tau = 12)), list("tlag"), "`tlag` must be finite")
err("dose_negative_steady", "domain", "MOD-MD-12, MOD-GEN-04", "pk1.iv_bolus", -1, c(P1, list(tau = 6)), list("dose"), "the dose must be >= 0")

ids <- vapply(errors, function(e) e$id, "")
check(!any(duplicated(ids)), "duplicate error ids: ", paste(ids[duplicated(ids)], collapse = ", "))
errors <- lapply(errors, function(e) {
  if (length(e$parameters) == 0L) e$parameters <- structure(list(), names = character(0))
  e
})

# ---------------------------------------------------------------- run
if (ONLY == "") {
  old <- list.files(exp_dir, full.names = TRUE)
  if (length(old) > 0) invisible(file.remove(old))
}
check(!any(duplicated(vapply(case_registry, function(c) paste(c$model, c$name), ""))), "duplicate case names")
done <- list()
for (cs in case_registry) {
  nm <- paste0(sub("\\.", "_", cs$model), "_", cs$name)
  if (ONLY != "" && !grepl(ONLY, nm)) next
  done[[length(done) + 1L]] <- write_case(cs)
}
total <- sum(vapply(done, function(d) d$n, 0))
cat(sprintf("value cases: %d, %d values\n", length(done), total))

if (ONLY == "") {
  base <- "model_md_errors"
  write_lines_lf(c("subject,parameter,value", sprintf("%s,result,", ids)), file.path(exp_dir, paste0(base, ".csv")))
  write_lines_lf(jenc(list(
    schema = 1L, kind = "model_errors", case = base,
    generated_by = "oracle/scripts/models_md.R",
    note = "inputs the multiple-dosing engine must refuse (MOD-MD-12); the expected table has one not-available row (empty value) per case, the reason is in `reason`, the rule in `spec`, and the readable message must contain, for each inner list of `message_contains`, at least one of its words (lower case). The regimen is encoded in `parameters` as in the case options: `tau`, `n_doses`, and for a schedule `dose_time[i]`, `dose_amount[i]`, `dose_dur[i]` (i from 0); the encoding is provisional until the engine card defines the interface (the test has one helper to change)",
    groups = list("tau", "tau_below_dur", "times", "n_doses", "schedule", "lag", "domain"),
    cases = errors,
    versions = versions,
    n_values = length(errors)
  )), file.path(exp_dir, paste0(base, ".options.json")))
  cat(sprintf("error cases: %d\n", length(errors)))

  # the list of cases for the engine test
  case_list <- function(names) paste0("    \"", names, "\",", collapse = "\n")
  macro_list <- function(names) paste0("    ", names, ",", collapse = "\n")
  sched_names <- vapply(Filter(function(d) d$kind != "steady_state", done), function(d) d$name, "")
  steady_names <- vapply(Filter(function(d) d$kind == "steady_state", done), function(d) d$name, "")
  write_lines_lf(c(
    "// Generated by oracle/scripts/models_md.R; do not edit by hand.",
    "const SCHEDULE_CASES: &[&str] = &[", case_list(sched_names), "];",
    "const STEADY_CASES: &[&str] = &[", case_list(steady_names), "];",
    "schedule_tests! {", macro_list(sched_names), "}",
    "steady_tests! {", macro_list(steady_names), "}"),
    file.path(root, "crates", "caladrius-models", "tests", "oracle_models_md.cases"))
  cat(sprintf("test list: %d schedule or regular cases, %d steady-state cases\n", length(sched_names), length(steady_names)))
}
