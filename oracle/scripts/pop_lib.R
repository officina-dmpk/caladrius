# Caladrius population-fit oracle (task T-152): the shared 256-bit machinery.
#
# Sourced by pop_exact.R, pop_ofv.R and the other pop_*.R scripts; it writes nothing by itself.
# Everything here is written from specs/population.md (POP-*) and the formulas of its sections, in
# Rmpfr at 256 bits, with no code from the engine and no population software:
#
#   * second-order "jets" (value, gradient, Hessian with respect to the q random effects) with
#     exact derivatives, so that the conditional mode, the Hessian of the log-likelihood and the
#     gradient of the objective are known to 256 bits, without finite differences;
#   * the structural models of specs/models.md written in closed form (identity_linear for the
#     test-only model of POP-MOD-05, pk1.iv_bolus, pk1.oral_1, pk2.iv_bolus in the clearance set);
#   * the objective forms of POP-OBJ-02 to POP-OBJ-04 (linearised, FO, three Laplace forms);
#   * the exact marginal likelihood by trapezoidal quadrature for q = 1, with its error bound;
#   * the diagnostics of POP-DIA-01 to POP-DIA-04.
#
# Inputs are decimal strings (Mp() refuses a non-integer double) so that no double is ever
# converted to 256 bits as if it were the decimal number it prints as.

suppressPackageStartupMessages({
  library(Rmpfr)
  library(jsonlite)
})

BITS <- 256L

Mp <- function(x) {
  if (inherits(x, "mpfr")) return(x)
  if (is.character(x)) return(mpfr(x, BITS))
  if (is.numeric(x) && all(x == round(x)) && all(abs(x) < 2^53)) return(mpfr(x, BITS))
  stop("Mp: a non-integer double was given; give a decimal string")
}
PI_MP <- Const("pi", BITS)
LN_2PI <- log(2 * PI_MP)
ZERO <- mpfr(0, BITS)
ONE <- mpfr(1, BITS)

# ------------------------------------------------------------------ small dense algebra
# A matrix is a list of rows, each an mpfr vector.
mat_zero <- function(n, m) lapply(seq_len(n), function(i) mpfr(rep(0, m), BITS))
mat_ident <- function(n) { A <- mat_zero(n, n); for (i in seq_len(n)) A[[i]][i] <- ONE; A }
mat_from_strings <- function(rows) lapply(rows, function(r) mpfr(unlist(r), BITS))
mat_vec <- function(A, x) { out <- mpfr(rep(0, length(A)), BITS); for (i in seq_along(A)) out[i] <- sum(A[[i]] * x); out }
mat_t <- function(A) { n <- length(A); m <- length(A[[1]]); B <- mat_zero(m, n)
  for (i in seq_len(n)) for (j in seq_len(m)) B[[j]][i] <- A[[i]][j]; B }
mat_mul <- function(A, B) { Bt <- mat_t(B); C <- mat_zero(length(A), length(B[[1]]))
  for (i in seq_along(A)) for (j in seq_along(Bt)) C[[i]][j] <- sum(A[[i]] * Bt[[j]]); C }
mat_add <- function(A, B) lapply(seq_along(A), function(i) A[[i]] + B[[i]])
mat_scale <- function(A, s) lapply(A, function(r) r * s)
quad_form <- function(x, A) sum(x * mat_vec(A, x))

chol_mp <- function(A) {
  n <- length(A); L <- mat_zero(n, n)
  for (j in seq_len(n)) {
    s <- A[[j]][j]
    for (k in seq_len(j - 1L)) s <- s - L[[j]][k] * L[[j]][k]
    if (!(s > 0)) stop("chol_mp: matrix not positive definite")
    d <- sqrt(s); L[[j]][j] <- d
    for (i in seq_len(n)) if (i > j) {
      s <- A[[i]][j]
      for (k in seq_len(j - 1L)) s <- s - L[[i]][k] * L[[j]][k]
      L[[i]][j] <- s / d
    }
  }
  L
}
fwd_mp <- function(L, b) {
  n <- length(b); x <- mpfr(rep(0, n), BITS)
  for (i in seq_len(n)) { s <- b[i]; for (k in seq_len(i - 1L)) s <- s - L[[i]][k] * x[k]; x[i] <- s / L[[i]][i] }
  x
}
back_mp <- function(L, b) { # solves t(L) x = b
  n <- length(b); x <- mpfr(rep(0, n), BITS)
  for (i in rev(seq_len(n))) { s <- b[i]; for (k in seq_len(n)) if (k > i) s <- s - L[[k]][i] * x[k]; x[i] <- s / L[[i]][i] }
  x
}
chol_solve <- function(L, b) back_mp(L, fwd_mp(L, b))
logdet_chol <- function(L) { s <- ZERO; for (i in seq_along(L)) s <- s + 2 * log(L[[i]][i]); s }
solve_mp <- function(A, b) { # Gauss elimination with partial pivoting
  n <- length(b)
  Mx <- lapply(seq_len(n), function(i) c(A[[i]], b[i]))
  for (cc in seq_len(n)) {
    cand <- cc:n
    p <- cand[which.max(vapply(cand, function(r) abs(asNumeric(Mx[[r]][cc])), 0))]
    if (p != cc) { tmp <- Mx[[cc]]; Mx[[cc]] <- Mx[[p]]; Mx[[p]] <- tmp }
    if (asNumeric(abs(Mx[[cc]][cc])) == 0) stop("solve_mp: singular")
    for (r in seq_len(n)) if (r != cc) {
      f <- Mx[[r]][cc] / Mx[[cc]][cc]
      Mx[[r]] <- Mx[[r]] - f * Mx[[cc]]
    }
  }
  x <- mpfr(rep(0, n), BITS)
  for (i in seq_len(n)) x[i] <- Mx[[i]][n + 1L] / Mx[[i]][i]
  x
}
inv_mp <- function(A) { n <- length(A); cols <- lapply(seq_len(n), function(j) { e <- mpfr(rep(0, n), BITS); e[j] <- ONE; solve_mp(A, e) })
  mat_t(cols) }

# ------------------------------------------------------------------ jets
# A jet over q variables: value v (mpfr), gradient g (list of q), Hessian H (list of q*q, row-major).
jet <- function(v, g, H) structure(list(v = v, g = g, H = H), class = "jet")
is_jet <- function(x) inherits(x, "jet")
jconst <- function(v, q) jet(Mp(v), rep(list(ZERO), q), rep(list(ZERO), q * q))
jvar <- function(i, v, q) { g <- rep(list(ZERO), q); g[[i]] <- ONE; jet(Mp(v), g, rep(list(ZERO), q * q)) }
jq <- function(a) length(a$g)
jchain <- function(a, f0, f1, f2) {
  q <- jq(a); H <- vector("list", q * q)
  for (i in seq_len(q)) for (j in seq_len(q)) { k <- (i - 1L) * q + j; H[[k]] <- f1 * a$H[[k]] + f2 * a$g[[i]] * a$g[[j]] }
  jet(f0, lapply(a$g, function(x) f1 * x), H)
}
jmul <- function(a, b) {
  q <- jq(a); H <- vector("list", q * q)
  for (i in seq_len(q)) for (j in seq_len(q)) { k <- (i - 1L) * q + j
    H[[k]] <- a$H[[k]] * b$v + b$H[[k]] * a$v + a$g[[i]] * b$g[[j]] + b$g[[i]] * a$g[[j]] }
  jet(a$v * b$v, lapply(seq_len(q), function(i) a$g[[i]] * b$v + a$v * b$g[[i]]), H)
}
jinv <- function(b) { v <- b$v; jchain(b, 1 / v, -1 / (v * v), 2 / (v * v * v)) }
Ops.jet <- function(e1, e2) {
  if (missing(e2)) { if (.Generic == "-") return(jet(-e1$v, lapply(e1$g, function(x) -x), lapply(e1$H, function(x) -x))); stop("unary op") }
  if (!is_jet(e1) || !is_jet(e2)) stop("jet arithmetic needs jets on both sides; lift constants with K()")
  switch(.Generic,
    "+" = jet(e1$v + e2$v, Map(`+`, e1$g, e2$g), Map(`+`, e1$H, e2$H)),
    "-" = jet(e1$v - e2$v, Map(`-`, e1$g, e2$g), Map(`-`, e1$H, e2$H)),
    "*" = jmul(e1, e2),
    "/" = jmul(e1, jinv(e2)),
    stop("jet op not supported: ", .Generic))
}
Math.jet <- function(x, ...) {
  v <- x$v
  switch(.Generic,
    exp = { e <- exp(v); jchain(x, e, e, e) },
    log = jchain(x, log(v), 1 / v, -1 / (v * v)),
    sqrt = { s <- sqrt(v); jchain(x, s, 1 / (2 * s), -1 / (4 * s * v)) },
    stop("jet function not supported: ", .Generic))
}
jsum <- function(lst) Reduce(`+`, lst)
jabs <- function(x) if (is_jet(x)) { if (x$v < 0) -x else x } else abs(x)
jval <- function(x) if (is_jet(x)) x$v else x

# K(): lift a constant for the evaluation mode (jets over q variables, or plain mpfr).
make_K <- function(q) if (is.null(q)) function(x) Mp(x) else function(x) jconst(Mp(x), q)

# ------------------------------------------------------------------ structural models
# P: named list of parameter values (jets or mpfr); dose, t lifted by K; K lifts literals.
model_iv_bolus <- function(P, D, t, K) { k <- P$cl / P$v; (D / P$v) * exp(-(k * t)) }
model_oral_1 <- function(P, D, t, K) {
  k <- P$cl / P$v; ka <- P$ka
  if (abs(asNumeric(jval(ka) - jval(k))) < 1e-9 * asNumeric(jval(k))) stop("oral_1: ka equals k; the case must avoid it")
  (D * ka / (P$v * (ka - k))) * (exp(-(k * t)) - exp(-(ka * t)))
}
model_pk2_iv_bolus <- function(P, D, t, K) {
  k10 <- P$cl / P$vc; k12 <- P$q / P$vc; k21 <- P$q / P$vp
  S <- k10 + k12 + k21
  disc <- (k10 - k21) * (k10 - k21) + k12 * (k12 + K(2) * k10 + K(2) * k21)
  r <- sqrt(disc); al <- (S + r) / K(2); be <- (k10 * k21) / al
  wa <- (al - k21) / (al - be); wb <- (k21 - be) / (al - be)
  (D / P$vc) * (wa * exp(-(al * t)) + wb * exp(-(be * t)))
}
MODEL_FNS <- list(`pk1.iv_bolus` = model_iv_bolus, `pk1.oral_1` = model_oral_1, `pk2.iv_bolus` = model_pk2_iv_bolus)

# spec: list(model, theta (named list of decimal strings), re (character vector of parameter names
# with a random effect, in the order of the columns of Omega), omega (list of rows of strings),
# error_model, xi (list a, b of decimal strings), z_powers (identity_linear only))
make_spec <- function(model, theta, re, omega, error_model, xi, z_powers = NULL) {
  om <- mat_from_strings(omega)
  list(model = model, theta = lapply(theta, Mp), re = re, q = length(re), omega = om,
       omega_chol = chol_mp(om), omega_inv = inv_mp(om),
       error_model = error_model, xi = list(a = Mp(xi$a %||% "0"), b = Mp(xi$b %||% "0")),
       z_powers = z_powers)
}
`%||%` <- function(a, b) if (is.null(a)) b else a

# prediction vector for one subject at the random effects eta (list of q jets or mpfr)
predict_subject <- function(spec, subj, eta, K) {
  D <- K(subj$dose)
  if (spec$model == "identity_linear") {
    th <- lapply(spec$theta, K)
    return(lapply(subj$time, function(t) {
      tt <- K(t); a <- th[[1]] + th[[2]] * tt
      for (k in seq_len(spec$q)) {
        pw <- spec$z_powers[k]; z <- if (pw == 0) K(1) else { z <- tt; if (pw > 1) for (u in 2:pw) z <- z * tt; z }
        a <- a + z * eta[[k]]
      }
      a }))
  }
  P <- lapply(names(spec$theta), function(nm) {
    th <- K(spec$theta[[nm]]); idx <- match(nm, spec$re)
    if (is.na(idx)) th else th * exp(eta[[idx]])
  })
  names(P) <- names(spec$theta)
  f <- MODEL_FNS[[spec$model]]
  lapply(subj$time, function(t) f(P, D, K(t), K))
}
sigma2_of <- function(spec, yhat, K) {
  a <- K(spec$xi$a); b <- K(spec$xi$b)
  switch(spec$error_model,
    additive = a * a,
    proportional = b * b * yhat * yhat,
    combined = a * a + b * b * yhat * yhat,
    combined_sd = { s <- a + b * jabs(yhat); s * s },
    stop("unknown error model"))
}

# eta as jets over the q variables
eta_jets <- function(vals, q) lapply(seq_len(q), function(i) jvar(i, vals[[i]], q))

# ell = sum(ln s2 + r^2/s2), pen = eta' Omega^-1 eta, with the prediction jets; sig2_fixed: NULL or a
# list of mpfr (the variance held at a given value, no interaction)
subject_parts <- function(spec, subj, eta_vals, sig2_fixed = NULL) {
  q <- spec$q; K <- make_K(q); eta <- eta_jets(eta_vals, q)
  yh <- predict_subject(spec, subj, eta, K)
  ell <- NULL; s2l <- vector("list", length(yh))
  for (j in seq_along(yh)) {
    r <- K(subj$y[[j]]) - yh[[j]]
    s2 <- if (is.null(sig2_fixed)) sigma2_of(spec, yh[[j]], K) else K(sig2_fixed[[j]])
    s2l[[j]] <- s2
    term <- log(s2) + r * r / s2
    ell <- if (is.null(ell)) term else ell + term
  }
  pen <- NULL
  for (k in seq_len(q)) for (l in seq_len(q)) {
    term <- K(spec$omega_inv[[k]][l]) * eta[[k]] * eta[[l]]
    pen <- if (is.null(pen)) term else pen + term
  }
  list(yhat = yh, s2 = s2l, ell = ell, pen = pen, h = ell + pen)
}
hess_matrix <- function(j) { q <- jq(j); lapply(seq_len(q), function(i) mpfr(unlist(lapply(seq_len(q), function(k) j$H[[(i - 1L) * q + k]])), BITS)) }
grad_vec <- function(j) mpfr(unlist(lapply(j$g, function(x) x)), BITS)
mpfr_list <- function(v) lapply(seq_along(v), function(i) v[i])

# conditional mode: Newton on h with exact jets and a backtracking line search; stops at
# max|grad| < tol (default 1e-70, about 2^-232)
find_mode <- function(spec, subj, start = NULL, sig2_fixed = NULL, tol = mpfr("1e-70", BITS), maxit = 200L) {
  q <- spec$q
  eta <- if (is.null(start)) rep(list(ZERO), q) else lapply(start, Mp)
  P <- subject_parts(spec, subj, eta, sig2_fixed); h <- P$h
  for (it in seq_len(maxit)) {
    g <- grad_vec(h); gmax <- max(abs(g))
    if (gmax < tol) return(list(eta = eta, parts = P, iterations = it - 1L, gradmax = gmax))
    H <- hess_matrix(h)
    # a Newton step where the Hessian of h is positive definite; where it is not (far from the mode, with an
    # interaction model), a Levenberg step: the smallest ridge lam*I, lam doubling from 1e-3 (1 + max|H|), that makes it so
    pd <- !inherits(try(chol_mp(H), silent = TRUE), "try-error")
    if (pd) step <- solve_mp(H, g) else {
      lam <- mpfr("1e-3", BITS) * (1 + max(vapply(H, function(r) asNumeric(max(abs(r))), 0)))
      repeat {
        Hr <- lapply(seq_len(q), function(i) { r <- H[[i]]; r[i] <- r[i] + lam; r })
        if (!inherits(try(chol_mp(Hr), silent = TRUE), "try-error")) break
        lam <- lam * 2
      }
      step <- solve_mp(Hr, g)
    }
    lam <- ONE; ok <- FALSE
    for (bt in 1:80) {
      trial <- lapply(seq_len(q), function(i) eta[[i]] - lam * step[i])
      Pn <- tryCatch(subject_parts(spec, subj, trial, sig2_fixed), error = function(e) NULL)
      if (!is.null(Pn) && (Pn$h$v <= h$v + abs(h$v) * mpfr("1e-60", BITS) || asNumeric(gmax) < 1e-30)) { ok <- TRUE; break }
      lam <- lam / 2
    }
    if (!ok) stop("find_mode: line search failed")
    eta <- trial; P <- Pn; h <- P$h
  }
  stop("find_mode: no convergence")
}

# --------------------------------------------------------------- objective forms
# Pieces at a given eta (the mode for the FOCEI forms, 0 for FO).
form_pieces <- function(spec, subj, eta_vals, sig2_fixed = NULL) {
  P <- subject_parts(spec, subj, eta_vals, sig2_fixed); q <- spec$q; n <- length(subj$time)
  G <- lapply(seq_len(n), function(j) mpfr(unlist(P$yhat[[j]]$g), BITS))     # n rows of q
  yh <- mpfr(unlist(lapply(P$yhat, function(x) x$v)), BITS)
  s2 <- mpfr(unlist(lapply(P$s2, function(x) x$v)), BITS)
  ys <- mpfr(unlist(lapply(subj$y, Mp)), BITS)
  list(parts = P, G = G, yhat = yh, s2 = s2, y = ys, n = n, q = q, eta = mpfr(unlist(eta_vals), BITS))
}
# POP-OBJ-02 / POP-OBJ-03: E = yhat - G eta, C = G Omega G' + diag(s2); full convention
lin_form <- function(spec, pc) {
  n <- pc$n; E <- pc$yhat - mpfr(vapply(seq_len(n), function(j) asNumeric(ZERO), 0), BITS)
  Geta <- mpfr(rep(0, n), BITS); for (j in seq_len(n)) Geta[j] <- sum(pc$G[[j]] * pc$eta)
  E <- pc$yhat - Geta
  GO <- lapply(seq_len(n), function(j) mat_vec(spec$omega, pc$G[[j]]))
  C <- mat_zero(n, n)
  for (i in seq_len(n)) for (j in seq_len(n)) C[[i]][j] <- sum(pc$G[[i]] * GO[[j]]) + (if (i == j) pc$s2[i] else ZERO)
  L <- chol_mp(C); rho <- pc$y - E; w <- fwd_mp(L, rho)
  list(ofv = n * LN_2PI + logdet_chol(L) + sum(w * w), E = E, C = C, L = L, rho = rho, w = w, logdet = logdet_chol(L), dist = sum(w * w))
}
# POP-OBJ-04 Laplace forms at the mode
laplace_forms <- function(spec, pc) {
  q <- pc$q; n <- pc$n; P <- pc$parts
  ell <- P$ell$v; pen <- P$pen$v
  Anv <- mat_zero(q, q); Aex <- mat_zero(q, q)
  for (j in seq_len(n)) {
    gj <- pc$G[[j]]; s2 <- pc$s2[j]; ds2 <- mpfr(unlist(P$s2[[j]]$g), BITS)
    for (k in seq_len(q)) for (l in seq_len(q)) {
      Anv[[k]][l] <- Anv[[k]][l] + gj[k] * gj[l] / s2
      Aex[[k]][l] <- Aex[[k]][l] + gj[k] * gj[l] / s2 + ds2[k] * ds2[l] / (2 * s2 * s2)
    }
  }
  Afull <- mat_scale(hess_matrix(P$ell), mpfr("0.5", BITS))
  ldo <- logdet_chol(spec$omega_chol)
  one <- function(A) n * LN_2PI + ell + pen + ldo + logdet_chol(chol_mp(mat_add(A, spec$omega_inv)))
  list(laplace_no_variance = one(Anv), laplace_expected = one(Aex), laplace_full = one(Afull))
}

# ---------------------------------------------------------------- exact marginal, q = 1
# L_i = integral of lik(eta) N(eta; 0, omega) d eta, by the trapezoidal rule on [-cut*sd, cut*sd]
# with step h and with h/2 (the nodes of h are every second node of h/2). The integrand is analytic
# and decays like a Gaussian, so the trapezoidal rule converges geometrically and
# |T(h)/T(h/2) - 1| bounds the relative error of T(h/2) (reported as discretisation_rel). The
# truncation is bounded by sup(lik) * erfc(cut / sqrt 2) / L, with sup(lik) taken as the largest
# likelihood value on the grid times e (one neper of margin); the ratio of the integrand at the edge
# to its maximum is reported (edge_log_ratio). The bounds are on L; the one on -2 ln L is twice them.
marginal_q1 <- function(spec, subj, h, cut = 13) {
  stopifnot(spec$q == 1L)
  om <- spec$omega[[1]][1]; wsd <- sqrt(om); n <- length(subj$time); K <- make_K(NULL)
  ys <- lapply(subj$y, Mp); hh <- h / 2
  nn <- as.integer(ceiling(asNumeric(cut * wsd / hh)))
  node <- function(i) {
    eta <- i * hh
    yh <- predict_subject(spec, subj, list(eta), K)
    s <- ZERO
    for (j in seq_len(n)) {
      s2 <- sigma2_of(spec, yh[[j]], K); r <- ys[[j]] - yh[[j]]
      s <- s - (LN_2PI + log(s2) + r * r / s2) / 2
    }
    c(s - eta * eta / (2 * om) - (LN_2PI + log(om)) / 2, s)
  }
  idx <- -nn:nn
  vals <- lapply(idx, node)
  Lm <- mpfr(rep(0, length(vals)), BITS)
  for (k in seq_along(vals)) Lm[k] <- vals[[k]][1]
  Ll <- mpfr(rep(0, length(vals)), BITS); for (k in seq_along(vals)) Ll[k] <- vals[[k]][2]
  mx <- max(Lm)
  fine <- mx + log(hh * sum(exp(Lm - mx)))
  even <- (idx %% 2L) == 0L
  coarse <- mx + log(h * sum(exp(Lm[even] - mx)))
  disc <- abs(expm1(coarse - fine))
  tail <- exp(max(Ll) + 1 - fine) * erfc(mpfr(cut, BITS) / sqrt(mpfr(2, BITS)))
  rel <- disc + tail
  list(ln_l = fine, ofv = -2 * fine, rel_error_bound = rel, ofv_error_bound = 2 * rel,
       discretisation_rel = disc, truncation_rel = tail, nodes = length(vals), step = hh,
       edge_log_ratio = max(Lm[1], Lm[length(Lm)]) - mx)
}


# ---------------------------------------------------------------- diagnostics (POP-DIA-*)
# CWRES in the three forms of POP-DIA-03 from the E and C of lin_form (the linearised form at the mode):
#   diagonal   (y - E)_j / sqrt(C_jj)
#   cholesky   L^-1 (y - E), C = L L' (the time order of the subject)
#   symmetric  C^(-1/2) (y - E), the symmetric inverse square root W = V diag(1/sqrt(lambda)) V' from a
#              cyclic Jacobi eigendecomposition of C carried out in 256 bits; the residual
#              max |W C W - I| is reported.
jacobi_eigen <- function(A, sweeps = 60L, tol = mpfr("1e-75", BITS)) {
  n <- length(A); V <- mat_ident(n); A <- lapply(A, function(r) r + 0)
  for (sw in seq_len(sweeps)) {
    off <- ZERO; for (i in seq_len(n)) for (j in seq_len(n)) if (i < j) off <- off + A[[i]][j] * A[[i]][j]
    if (sqrt(off) < tol) break
    for (p in seq_len(n - 1L)) for (q in (p + 1L):n) {
      apq <- A[[p]][q]
      if (abs(apq) < mpfr("1e-300", BITS)) next
      theta <- (A[[q]][q] - A[[p]][p]) / (2 * apq)
      t <- (if (theta >= 0) ONE else -ONE) / (abs(theta) + sqrt(theta * theta + 1))
      cs <- 1 / sqrt(t * t + 1); sn <- t * cs
      for (k in seq_len(n)) { akp <- A[[k]][p]; akq <- A[[k]][q]; A[[k]][p] <- cs * akp - sn * akq; A[[k]][q] <- sn * akp + cs * akq }
      for (k in seq_len(n)) { apk <- A[[p]][k]; aqk <- A[[q]][k]; A[[p]][k] <- cs * apk - sn * aqk; A[[q]][k] <- sn * apk + cs * aqk }
      for (k in seq_len(n)) { vkp <- V[[k]][p]; vkq <- V[[k]][q]; V[[k]][p] <- cs * vkp - sn * vkq; V[[k]][q] <- sn * vkp + cs * vkq }
    }
  }
  list(values = mpfr_(lapply(seq_len(n), function(i) A[[i]][i])), vectors = V)
}
mpfr_ <- function(lst) { out <- mpfr(rep(0, length(lst)), BITS); for (i in seq_along(lst)) out[i] <- lst[[i]]; out }
cwres_forms <- function(lf) {
  n <- length(lf$rho)
  diag_form <- mpfr_(lapply(seq_len(n), function(j) lf$rho[j] / sqrt(lf$C[[j]][j])))
  Csym <- lapply(seq_len(n), function(i) mpfr_(lapply(seq_len(n), function(j) (lf$C[[i]][j] + lf$C[[j]][i]) / 2)))
  ev <- jacobi_eigen(Csym)
  W <- mat_zero(n, n)
  for (i in seq_len(n)) for (j in seq_len(n)) { s <- ZERO; for (k in seq_len(n)) s <- s + ev$vectors[[i]][k] * ev$vectors[[j]][k] / sqrt(ev$values[k]); W[[i]][j] <- s }
  WCW <- mat_mul(mat_mul(W, lf$C), W)
  res <- max(vapply(seq_len(n), function(i) asNumeric(max(abs(WCW[[i]] - mat_ident(n)[[i]]))), 0))
  list(diagonal = diag_form, cholesky = lf$w, symmetric = mat_vec(W, lf$rho), symmetric_residual = res)
}

# ---------------------------------------------------------------- formatting / io
fmt_mp <- function(x, digits = 50) format(x, digits = digits)
fmt_vec <- function(v, digits = 50) vapply(seq_along(v), function(i) fmt_mp(v[i], digits), "")
dbl <- function(x) as.numeric(asNumeric(x))   # nearest double, correctly rounded
write_lines_lf <- function(lines, path) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
  con <- file(path, open = "wb"); on.exit(close(con)); writeLines(lines, con, sep = "\n", useBytes = TRUE)
}
write_json_lf <- function(obj, path, digits = 17) {
  write_lines_lf(toJSON(obj, auto_unbox = TRUE, pretty = TRUE, digits = digits, null = "null", na = "null"), path)
}
sha256_file <- function(path) digest::digest(file = path, algo = "sha256", serialize = FALSE)
versions_list <- function(extra = character()) {
  v <- list(R = R.version.string, Rmpfr = as.character(packageVersion("Rmpfr")), gmp = as.character(packageVersion("gmp")),
            jsonlite = as.character(packageVersion("jsonlite")), digest = as.character(packageVersion("digest")),
            RNGkind = paste(RNGkind(), collapse = "/"), precision_bits = BITS)
  for (p in extra) v[[p]] <- as.character(packageVersion(p))
  v
}
repo_root <- function() {
  args_all <- commandArgs(trailingOnly = FALSE)
  file_arg <- sub("^--file=", "", args_all[grepl("^--file=", args_all)])
  if (length(file_arg) == 1L) normalizePath(file.path(dirname(file_arg), "..", ".."), winslash = "/", mustWork = TRUE)
  else normalizePath(".", winslash = "/", mustWork = TRUE)
}

# ---------------------------------------------------------------- frozen sigma (POP-OBJ-04, OP-14)
# sigma^2 held fixed while the mode is found, then used in every form.
#   at = "pred0": sigma^2 at the population prediction yhat(0) (classical FOCE);
#   at = "mode":  sigma^2 at yhat(eta_hat), the fixed point (mode recomputed with the frozen sigma).
find_mode_frozen <- function(spec, subj, at = c("mode", "pred0"), tol = mpfr("1e-70", BITS)) {
  at <- match.arg(at); q <- spec$q
  P0 <- subject_parts(spec, subj, rep(list(ZERO), q))
  s2 <- lapply(P0$s2, function(x) x$v)
  m <- find_mode(spec, subj, sig2_fixed = s2)
  if (at == "pred0") return(c(m, list(sig2 = s2, fixed_point_iterations = 0L)))
  # The fixed point eta = argmin h(eta; s2 = s2(yhat(eta))): Newton on
  #   F(eta) = sum_j -2 r_j G_j / s2_j + 2 Omega^-1 eta   (s2_j evaluated at eta, held constant in the derivative of ell),
  # with the exact Jacobian  J_kl = sum_j -2 [ -G_jl G_jk / s2_j + r_j H_jkl / s2_j - r_j G_jk (ds2_j/deta_l) / s2_j^2 ] + 2 Omega^-1_kl.
  Fj <- function(eta) {
    P <- subject_parts(spec, subj, eta, NULL); n <- length(P$yhat)
    Fv <- mpfr(rep(0, q), BITS); J <- mat_zero(q, q)
    for (j in seq_len(n)) {
      r <- Mp(subj$y[[j]]) - P$yhat[[j]]$v; s2j <- P$s2[[j]]$v
      G <- mvec_(P$yhat[[j]]$g); ds2 <- mvec_(P$s2[[j]]$g)
      for (k in seq_len(q)) {
        Fv[k] <- Fv[k] - 2 * r * G[k] / s2j
        for (l in seq_len(q)) {
          Hkl <- P$yhat[[j]]$H[[(k - 1L) * q + l]]
          J[[k]][l] <- J[[k]][l] - 2 * (-G[l] * G[k] / s2j + r * Hkl / s2j - r * G[k] * ds2[l] / (s2j * s2j))
        }
      }
    }
    for (k in seq_len(q)) { acc <- ZERO; for (l in seq_len(q)) { acc <- acc + spec$omega_inv[[k]][l] * eta[[l]]; J[[k]][l] <- J[[k]][l] + 2 * spec$omega_inv[[k]][l] }
      Fv[k] <- Fv[k] + 2 * acc }
    list(F = Fv, J = J, parts = P)
  }
  eta <- m$eta; cur <- Fj(eta); its <- 0L
  for (it in 1:60) {
    fm <- max(abs(cur$F))
    if (fm < tol) break
    step <- solve_mp(cur$J, cur$F); lam <- ONE; ok <- FALSE
    for (bt in 1:60) {
      trial <- lapply(seq_len(q), function(i) eta[[i]] - lam * step[i])
      nx <- tryCatch(Fj(trial), error = function(e) NULL)
      if (!is.null(nx) && (max(abs(nx$F)) < fm || max(abs(nx$F)) < tol)) { ok <- TRUE; break }
      lam <- lam / 2
    }
    if (!ok) stop("find_mode_frozen: line search failed")
    eta <- trial; cur <- nx; its <- it
  }
  if (max(abs(cur$F)) >= tol) stop("find_mode_frozen: fixed point not reached")
  K <- make_K(q)
  s2 <- lapply(cur$parts$yhat, function(y) sigma2_of(spec, y, K)$v)
  m <- find_mode(spec, subj, start = eta, sig2_fixed = s2, tol = mpfr("1e-68", BITS))
  c(m, list(sig2 = s2, fixed_point_iterations = its, fixed_point_residual = max(abs(cur$F))))
}
mvec_ <- function(lst) { out <- mpfr(rep(0, length(lst)), BITS); for (i in seq_along(lst)) out[i] <- lst[[i]]; out }

# ---------------------------------------------------------------- double-precision simulation helpers
# Used only to GENERATE data (seeded); the data enter the 256-bit computation as decimal strings.
pred_dbl <- function(model, P, D, t) {
  switch(model,
    `pk1.iv_bolus` = (D / P$v) * exp(-(P$cl / P$v) * t),
    `pk1.oral_1` = { k <- P$cl / P$v; D * P$ka / (P$v * (P$ka - k)) * (exp(-k * t) - exp(-P$ka * t)) },
    `pk2.iv_bolus` = {
      k10 <- P$cl / P$vc; k12 <- P$q / P$vc; k21 <- P$q / P$vp; S <- k10 + k12 + k21
      al <- (S + sqrt((k10 - k21)^2 + k12 * (k12 + 2 * k10 + 2 * k21))) / 2; be <- k10 * k21 / al
      (D / P$vc) * (((al - k21) / (al - be)) * exp(-al * t) + ((k21 - be) / (al - be)) * exp(-be * t))
    },
    stop("pred_dbl: unknown model"))
}
sd_dbl <- function(error_model, a, b, yhat) {
  switch(error_model, additive = rep(a, length(yhat)), proportional = b * abs(yhat),
         combined = sqrt(a^2 + b^2 * yhat^2), combined_sd = a + b * abs(yhat), stop("error model"))
}
dec_str <- function(x, sig = 3) vapply(x, function(v) { s <- format(signif(v, sig), scientific = FALSE, trim = TRUE, digits = 15); s }, "")
# One simulated dataset: m subjects, the same times for every subject unless times_by_subject is given.
simulate_subjects <- function(model, theta, re, omega, error_model, xi, dose, times, m, seed, sig = 3, prefix = "S",
                              times_by_subject = NULL) {
  set.seed(seed, kind = "Mersenne-Twister", normal.kind = "Inversion", sample.kind = "Rejection")
  th <- lapply(theta, as.numeric); Om <- matrix(as.numeric(unlist(omega)), length(re), length(re), byrow = TRUE)
  Lc <- t(chol(Om)); out <- vector("list", m)
  for (i in seq_len(m)) {
    tm <- if (is.null(times_by_subject)) times else times_by_subject[[i]]
    eta <- as.vector(Lc %*% rnorm(length(re))); P <- th
    for (k in seq_along(re)) P[[re[k]]] <- th[[re[k]]] * exp(eta[k])
    tn <- as.numeric(tm); yh <- pred_dbl(model, P, as.numeric(dose), tn)
    y <- yh + sd_dbl(error_model, as.numeric(xi$a %||% 0), as.numeric(xi$b %||% 0), yh) * rnorm(length(tn))
    out[[i]] <- list(id = paste0(prefix, i), time = as.character(tm), y = dec_str(y, sig), dose = as.character(dose))
  }
  out
}
