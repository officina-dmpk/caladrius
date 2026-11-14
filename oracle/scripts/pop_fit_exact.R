# Caladrius population-fit oracle, card T-168 (POP-ORA-01, fourth family): the exact fit-level oracle for the
# balanced linear case.
#
# The model is the test-only `identity_linear` of POP-MOD-05 with an additive error, so FOCEI, FOCE and FO are all the
# exact marginal likelihood of a linear mixed model y_i ~ N(a_i, Z Omega Z' + sigma^2 I). With every subject on the same
# times and the columns of Z in the span of [1, t], the generalised least squares estimate of (theta_1, theta_2) is the
# pooled ordinary least squares fit, and the maximum is known in closed form (q = 1; q = 2 with a block Omega) or by a
# search over (Omega, sigma^2) with a few parameters.
#
# Three references, independent of each other, for every case (specs/population.md POP-ORA-01, fourth family):
#   (1) 256 bits (Rmpfr), every input parsed from its decimal string: the closed form where there is one; a Newton
#       polish on exact derivatives (second-order jets of pop_lib.R) of the exact objective for the cases that have none
#       (q = 2 diagonal, the boundaries). Every reference point is checked to be stationary (gradient of the free
#       parameters) and, on a boundary, to satisfy the sign conditions of the constrained maximum.
#   (2) nlme::lme(method = "ML") (a fit of the same likelihood by another method), at its defaults and at a tight setting.
#   (3) a direct maximisation, in double precision, of the objective of POP-OBJ-02 written out from the covariance matrix
#       of one subject (BFGS, Nelder-Mead and L-BFGS-B, several starts).
# The worst disagreement of each reference with the 256-bit point is recorded per case. The standard errors (the three
# kinds of POP-COV-01) are recorded from the exact derivatives and are not asserted.
#
# The convention of the OFV everywhere is `full` (POP-OBJ-05); `no_2pi` is the same number minus N ln 2 pi.
#
# Run: Rscript oracle/scripts/pop_fit_exact.R   (writes oracle/expected/pop/fit_exact/*.json, LF endings)
source(file.path(dirname(sub("^--file=", "", grep("^--file=", commandArgs(FALSE), value = TRUE)[1])), "pop_lib.R"))
suppressPackageStartupMessages(library(nlme))
ROOT <- repo_root()
OUT <- file.path(ROOT, "oracle", "expected", "pop", "fit_exact")
dir.create(OUT, recursive = TRUE, showWarnings = FALSE)
unlink(list.files(OUT, pattern = "\\.json$", full.names = TRUE))

SIG <- 50L                        # significant digits written for every 256-bit value
S <- function(x) fmt_mp(x, SIG)
Mstr <- function(x) mpfr(sprintf("%.17g", x), BITS)       # a double, as the decimal number it is printed as at 17 digits
arr <- function(x) as.list(x)
BOUNDARY_ZERO <- 1e-7             # a variance below this in the double-precision search is taken to be held on its bound

# ============================================================================================ the designs
TIMES5 <- c("0.5", "1", "2", "4", "8")
TIMES6 <- c("0.5", "1", "2", "4", "6", "8")
DESIGNS <- list(
  list(id = "fit_q1_h38", shape = "q1", z_powers = 0L, m = 6L, times = TIMES5, base_seed = 38001L, n_seeds = 1L, want = "interior",
       truth = list(theta = c("10", "-0.8"), omega = list(c("4")), sigma = "0.9"), generator = "h38",
       description = "q = 1 (random intercept), m = 6, the data of H38 (specs/sources.md): the interior maximum, closed form",
       far = list(theta = c("3", "0.5"), omega = list(c("0.5")), sigma = "2")),
  list(id = "fit_q1_boundary", shape = "q1", z_powers = 0L, m = 8L, times = TIMES5, base_seed = 39001L, n_seeds = 500L, want = "boundary",
       truth = list(theta = c("10", "-0.8"), omega = list(c("0.02")), sigma = "1.2"), generator = "lme",
       description = "q = 1 with lambda-hat <= sigma-hat^2: omega-hat = 0 and sigma-hat^2 = (W + B) / N, the maximum is on the boundary",
       far = list(theta = c("3", "0.5"), omega = list(c("2")), sigma = "3")),
  list(id = "fit_q2_block", shape = "block", z_powers = c(0L, 1L), m = 12L, times = TIMES6, base_seed = 40001L, n_seeds = 500L, want = "interior",
       truth = list(theta = c("10", "-0.8"), omega = list(c("4", "-0.3"), c("-0.3", "0.09")), sigma = "0.9"), generator = "lme",
       description = "q = 2 (intercept and slope), block Omega (correlation -0.5), m = 12: the interior maximum, closed form (specs: assumed until this run)",
       far = list(theta = c("3", "0.5"), omega = list(c("1", "0.1"), c("0.1", "1")), sigma = "2")),
  list(id = "fit_q2_diag", shape = "diag", z_powers = c(0L, 1L), m = 12L, times = TIMES6, base_seed = 41001L, n_seeds = 500L, want = "interior",
       truth = list(theta = c("10", "-0.8"), omega = list(c("4", "0"), c("0", "0.09")), sigma = "0.9"), generator = "lme",
       description = "q = 2, diagonal Omega, m = 12: no closed form, the maximum of (omega_1, omega_2, sigma^2) polished at 256 bits",
       far = list(theta = c("3", "0.5"), omega = list(c("1", "0"), c("0", "1")), sigma = "2")),
  list(id = "fit_q2_diag_boundary", shape = "diag", z_powers = c(0L, 1L), m = 12L, times = TIMES6, base_seed = 42001L, n_seeds = 2000L, want = "boundary",
       truth = list(theta = c("10", "-0.8"), omega = list(c("4", "0"), c("0", "0.0004")), sigma = "1"), generator = "lme",
       description = "q = 2, diagonal Omega, slope variance on its bound (omega_2-hat = 0): no closed form for the constrained maximum, polished at 256 bits with the active set",
       far = list(theta = c("3", "0.5"), omega = list(c("1", "0"), c("0", "1")), sigma = "2")),
  list(id = "fit_q2_block_boundary", shape = "block", z_powers = c(0L, 1L), m = 12L, times = TIMES6, base_seed = 42001L, n_seeds = 2000L, want = "boundary",
       truth = list(theta = c("10", "-0.8"), omega = list(c("4", "0"), c("0", "0.0004")), sigma = "1"), generator = "lme",
       description = "q = 2, block Omega on the data of fit_q2_diag_boundary: the closed-form Omega-hat is not positive definite, the maximum is on the boundary of the cone",
       far = list(theta = c("3", "0.5"), omega = list(c("1", "0.1"), c("0.1", "1")), sigma = "2"))
)

# ---------------------------------------------------------------------------------------------- generating a dataset
# The generator of H38 (and of every case): set.seed(seed, "Mersenne-Twister", "Inversion", "Rejection"); for each
# subject in order, first the random effects (q = 1: rnorm(1, 0, sd); q = 2: L %*% rnorm(2), L the Cholesky factor
# of Omega), then the n errors rnorm(n, 0, sigma); y = theta1 + theta2 t + Z eta + error, rounded to three decimals.
generate <- function(d, seed) {
  set.seed(seed, kind = "Mersenne-Twister", normal.kind = "Inversion", sample.kind = "Rejection")
  tt <- as.numeric(d$times); n <- length(tt); q <- length(d$z_powers)
  th <- as.numeric(d$truth$theta); sg <- as.numeric(d$truth$sigma)
  Om <- matrix(as.numeric(unlist(d$truth$omega)), q, q, byrow = TRUE)
  Z <- sapply(d$z_powers, function(p) tt^p)
  Lc <- t(chol(Om)); subj <- vector("list", d$m)
  for (i in seq_len(d$m)) {
    eta <- if (q == 1L) rnorm(1, 0, sqrt(Om[1, 1])) else as.vector(Lc %*% rnorm(q))
    e <- rnorm(n, 0, sg)
    y <- round(th[1] + th[2] * tt + as.vector(Z %*% eta) + e, 3)
    subj[[i]] <- list(id = paste0("S", i), dose = "1", time = d$times, y = sprintf("%.3f", y))
  }
  subj
}

# ---------------------------------------------------------------------------------------------- double-precision helpers
design_dbl <- function(d, subj) {
  tt <- as.numeric(d$times); n <- length(tt)
  list(tt = tt, n = n, m = length(subj), q = length(d$z_powers), Z = sapply(d$z_powers, function(p) tt^p),
       Y = t(sapply(subj, function(s) as.numeric(s$y))), X = cbind(1, tt))
}
# The exact objective (full convention) written out from the covariance matrix of one subject; Omega is a q x q matrix.
neg2ll_dbl <- function(DD, th1, th2, Om, s2) {
  C <- DD$Z %*% Om %*% t(DD$Z) + s2 * diag(DD$n)
  Lc <- tryCatch(chol(C), error = function(e) NULL)
  if (is.null(Lc)) return(1e10)
  R <- DD$Y - matrix(th1 + th2 * DD$tt, DD$m, DD$n, byrow = TRUE)
  W <- backsolve(Lc, t(R), transpose = TRUE)
  DD$m * (DD$n * log(2 * pi) + 2 * sum(log(diag(Lc)))) + sum(W * W)
}
# Closed forms in double precision, used only to choose the seeds (the 256-bit code below is the reference).
classify_dbl <- function(d, subj) {
  DD <- design_dbl(d, subj); m <- DD$m; n <- DD$n; q <- DD$q
  beta <- solve(m * crossprod(DD$X), t(DD$X) %*% colSums(DD$Y))
  R <- DD$Y - matrix(as.vector(DD$X %*% beta), m, n, byrow = TRUE)
  if (q == 1L) {
    rb <- rowMeans(R); W <- sum((R - rb)^2); B <- n * sum(rb^2)
    s2 <- W / (m * (n - 1)); lam <- B / m
    return(list(block_interior = lam > s2, diag_interior = lam > s2, omega_bar = (lam - s2) / n))
  }
  ZtZ <- crossprod(DD$Z); G <- solve(ZtZ)
  Bm <- R %*% DD$Z %*% G                                       # row i is b_i
  Sm <- crossprod(Bm) / m
  Wp <- sum(R^2) - sum((Bm %*% ZtZ) * Bm)
  s2 <- Wp / (m * (n - q)); Om <- Sm - s2 * G
  block_ok <- Om[1, 1] > 0 && det(Om) > 0
  # diagonal: maximise the separable profile over (omega_1, omega_2, sigma^2) with omega >= 0
  prof <- function(p) { Sg <- diag(p[1:2]) + p[3] * G; m * ((n - q) * log(p[3]) + log(det(Sg))) + Wp / p[3] + m * sum(diag(solve(Sg, Sm))) }
  best <- NULL
  for (st in list(c(1, 0.1, 1), c(5, 1, 0.5), c(0.1, 0.01, 2))) {
    o <- tryCatch(optim(st, prof, method = "L-BFGS-B", lower = c(0, 0, 1e-8), control = list(maxit = 2000, factr = 10)), error = function(e) NULL)
    if (!is.null(o) && (is.null(best) || o$value < best$value)) best <- o
  }
  list(block_interior = block_ok, diag_interior = all(best$par[1:2] > BOUNDARY_ZERO), omega_bar = best$par[1:2])
}

# ============================================================================================ reference 1: 256 bits
# A matrix is a list of rows of mpfr numbers (pop_lib.R). Everything below is parsed from the decimal strings.
design_mp <- function(d, subj) {
  tt <- mpfr(d$times, BITS); n <- length(tt); q <- length(d$z_powers)
  Z <- lapply(seq_len(n), function(j) mvec_(lapply(d$z_powers, function(p) if (p == 0L) ONE else tt[j]^p)))
  Y <- lapply(subj, function(s) mpfr(s$y, BITS))
  list(tt = tt, n = n, m = length(subj), q = q, Z = Z, Y = Y)
}
# Pooled ordinary least squares on [1, t] (the generalised least squares estimate of a balanced design).
ols_mp <- function(DD) {
  m <- DD$m; n <- DD$n; tt <- DD$tt
  St <- sum(tt); Stt <- sum(tt * tt)
  A <- list(mvec_(list(mpfr(m * n, BITS), m * St)), mvec_(list(m * St, m * Stt)))
  sy <- ZERO; sty <- ZERO
  for (i in seq_len(m)) { sy <- sy + sum(DD$Y[[i]]); sty <- sty + sum(tt * DD$Y[[i]]) }
  beta <- solve_mp(A, mvec_(list(sy, sty)))
  R <- lapply(seq_len(m), function(i) DD$Y[[i]] - beta[1] - beta[2] * tt)
  list(beta = beta, R = R)
}
det2 <- function(A) A[[1]][1] * A[[2]][2] - A[[1]][2] * A[[2]][1]

# The closed form: q = 1 (interior and boundary) and q = 2 block (specs/population.md, POP-ORA-01 fourth family).
closed_form_mp <- function(shape, DD) {
  m <- DD$m; n <- DD$n; q <- DD$q; N <- m * n
  fit <- ols_mp(DD)
  if (shape == "q1") {
    rb <- lapply(fit$R, function(r) sum(r) / n)
    W <- ZERO; for (i in seq_len(m)) W <- W + sum((fit$R[[i]] - rb[[i]])^2)
    B <- ZERO; for (i in seq_len(m)) B <- B + n * rb[[i]]^2
    s2 <- W / (m * (n - 1)); lam <- B / m
    if (lam > s2) {
      om <- (lam - s2) / n
      ofv <- N * LN_2PI + m * (n - 1) * log(s2) + m * log(lam) + N
      return(list(kind = "interior", beta = fit$beta, omega = list(mvec_(list(om))), s2 = s2, ofv = ofv, W = W, B = B, lambda = lam))
    }
    s2b <- (W + B) / N
    return(list(kind = "boundary", beta = fit$beta, omega = list(mvec_(list(ZERO))), s2 = s2b, ofv = N * LN_2PI + N * log(s2b) + N, W = W, B = B, lambda = lam, sigma2_interior = s2))
  }
  if (shape == "block") {
    ZtZ <- mat_zero(q, q); for (j in seq_len(n)) for (a in seq_len(q)) for (b in seq_len(q)) ZtZ[[a]][b] <- ZtZ[[a]][b] + DD$Z[[j]][a] * DD$Z[[j]][b]
    G <- inv_mp(ZtZ)
    Bs <- lapply(seq_len(m), function(i) {
      zr <- mvec_(lapply(seq_len(q), function(a) sum(mvec_(lapply(seq_len(n), function(j) DD$Z[[j]][a] * fit$R[[i]][j])))))
      mat_vec(G, zr)
    })
    Sm <- mat_zero(q, q); Wp <- ZERO
    for (i in seq_len(m)) {
      for (a in seq_len(q)) for (b in seq_len(q)) Sm[[a]][b] <- Sm[[a]][b] + Bs[[i]][a] * Bs[[i]][b] / m
      Wp <- Wp + sum(fit$R[[i]] * fit$R[[i]]) - quad_form(Bs[[i]], ZtZ)
    }
    s2 <- Wp / (m * (n - q))
    Om <- lapply(seq_len(q), function(a) mvec_(lapply(seq_len(q), function(b) Sm[[a]][b] - s2 * G[[a]][b])))
    pd <- Om[[1]][1] > 0 && det2(Om) > 0
    out <- list(beta = fit$beta, omega = Om, s2 = s2, G = G, S = Sm, Wp = Wp, pd = pd, ZtZ = ZtZ)
    if (pd) {
      out$kind <- "interior"
      out$ofv <- N * LN_2PI + m * log(det2(ZtZ)) + m * log(det2(Sm)) + m * q + m * (n - q) * log(s2) + m * (n - q)
    } else out$kind <- "boundary_no_closed_form"
    return(out)
  }
  list(kind = "no_closed_form", beta = fit$beta)
}

# ---------------------------------------------------------------------------------------------- exact derivatives
# Parameter vector layouts (natural scale, the user's units of Omega and sigma = xi.a):
#   q1:    (theta1, theta2, omega, sigma)                 diag: (theta1, theta2, omega_1, omega_2, sigma)
#   block: (theta1, theta2, Omega_11, Omega_21, Omega_22, sigma)
param_names <- function(shape) switch(shape, q1 = c("theta1", "theta2", "omega_1", "sigma"), rank1 = c("theta1", "theta2", "ell_1", "ell_2", "sigma"),
  diag = c("theta1", "theta2", "omega_1", "omega_2", "sigma"), block = c("theta1", "theta2", "omega_11", "omega_21", "omega_22", "sigma"))
omega_index <- function(shape) switch(shape, q1 = 3L, diag = 3:4, block = 3:5)
omega_matrix_jets <- function(shape, P, K) {
  switch(shape, q1 = list(list(P[[3]])),
    diag = list(list(P[[3]], K(ZERO)), list(K(ZERO), P[[4]])),
    block = list(list(P[[3]], P[[4]]), list(P[[4]], P[[5]])),
    rank1 = list(list(P[[3]] * P[[3]], P[[3]] * P[[4]]), list(P[[3]] * P[[4]], P[[4]] * P[[4]])))
}
jet_chol <- function(C) {
  n <- length(C); L <- lapply(seq_len(n), function(i) vector("list", n))
  for (i in seq_len(n)) for (j in seq_len(i)) {
    s <- C[[i]][[j]]
    if (j > 1L) for (k in seq_len(j - 1L)) s <- s - L[[i]][[k]] * L[[j]][[k]]
    L[[i]][[j]] <- if (i == j) sqrt(s) else s / L[[j]][[j]]
  }
  L
}
# The per-subject objectives (full convention) as jets over the parameter vector, from the covariance matrix of one subject.
subject_jets <- function(shape, DD, par) {
  p <- length(par); K <- make_K(p)
  P <- lapply(seq_len(p), function(k) jvar(k, par[k], p))
  th1 <- P[[1]]; th2 <- P[[2]]; sg <- P[[p]]; s2 <- sg * sg
  Om <- omega_matrix_jets(shape, P, K); q <- DD$q; n <- DD$n
  Zj <- lapply(seq_len(n), function(j) lapply(seq_len(q), function(a) K(DD$Z[[j]][a])))
  C <- lapply(seq_len(n), function(a) lapply(seq_len(n), function(b) {
    s <- if (a == b) s2 else K(ZERO)
    for (k in seq_len(q)) for (l in seq_len(q)) s <- s + Zj[[a]][[k]] * Om[[k]][[l]] * Zj[[b]][[l]]
    s }))
  L <- jet_chol(C)
  logdet <- K(ZERO); for (i in seq_len(n)) logdet <- logdet + K(mpfr(2, BITS)) * log(L[[i]][[i]])
  lapply(seq_len(DD$m), function(i) {
    r <- lapply(seq_len(n), function(j) K(DD$Y[[i]][j]) - th1 - th2 * K(DD$tt[j]))
    w <- vector("list", n)
    for (a in seq_len(n)) {
      s <- r[[a]]
      if (a > 1L) for (k in seq_len(a - 1L)) s <- s - L[[a]][[k]] * w[[k]]
      w[[a]] <- s / L[[a]][[a]]
    }
    quad <- K(ZERO); for (a in seq_len(n)) quad <- quad + w[[a]] * w[[a]]
    K(n * LN_2PI) + logdet + quad
  })
}
total_jet <- function(sj) Reduce(`+`, sj)
sub_matrix <- function(Hm, idx) lapply(idx, function(i) mvec_(lapply(idx, function(j) Hm[[i]][j])))
# Newton polish on the free parameters (`free` indices of the parameter vector), the others held at their given values.
newton_polish <- function(shape, DD, par, free, maxit = 40L) {
  steps <- numeric(0)
  for (it in seq_len(maxit)) {
    tj <- total_jet(subject_jets(shape, DD, par))
    g <- grad_vec(tj); Hm <- hess_matrix(tj)
    step <- solve_mp(sub_matrix(Hm, free), g[free])
    par[free] <- par[free] - step
    steps <- c(steps, dbl(max(abs(step))))
    if (max(abs(step)) < mpfr("1e-70", BITS)) break
  }
  list(par = par, iterations = it, last_step = steps[length(steps)])
}

# ============================================================================================ reference 2: nlme::lme (ML)
# `getVarCov` and `sigma` carry full double precision (VarCorr prints 8 digits). OFV = -2 logLik, the `full` convention.
run_lme <- function(shape, d, subj, tight) {
  tt <- as.numeric(d$times); n <- length(tt)
  df <- data.frame(id = factor(rep(vapply(subj, function(s) s$id, ""), each = n), levels = vapply(subj, function(s) s$id, "")),
                   t = rep(tt, times = length(subj)), y = as.numeric(unlist(lapply(subj, function(s) s$y))))
  ctrl <- if (tight) lmeControl(maxIter = 1000, msMaxIter = 1000, msMaxEval = 5000, tolerance = 1e-10, msTol = 1e-14, niterEM = 100, returnObject = TRUE, opt = "nlminb")
          else lmeControl()
  random <- switch(shape, q1 = ~ 1 | id, diag = list(id = pdDiag(~ 1 + t)), block = list(id = pdSymm(~ 1 + t)))
  res <- tryCatch({
    withCallingHandlers(
      fit <- lme(y ~ t, data = df, random = random, method = "ML", control = ctrl),
      warning = function(w) invokeRestart("muffleWarning"))
    V <- getVarCov(fit)
    list(ok = TRUE, theta = as.numeric(fixef(fit)), omega = unclass(V)[seq_len(nrow(V)), seq_len(nrow(V)), drop = FALSE],
         sigma = fit$sigma, ofv = -2 * as.numeric(logLik(fit)), message = "")
  }, error = function(e) list(ok = FALSE, message = conditionMessage(e)))
  res
}

# ============================================================================================ reference 3: direct maximisation
# The exact objective of POP-OBJ-02 written out from the covariance matrix of one subject (neg2ll_dbl), maximised in double
# precision (minimised as -2 log L) from several starts, with three parameterisations:
#   logs:     log of the variances (q1, diag) or the Cholesky factor of Omega (block), log sigma^2; BFGS, then Nelder-Mead, then BFGS
#   natural:  the variances themselves (q1, diag), bounded below by 0 (L-BFGS-B), sigma^2 bounded below by 1e-10
# The parameter vector of the search is (theta1, theta2, Omega parameters, log sigma^2 or sigma^2).
direct_max <- function(shape, d, subj) {
  DD <- design_dbl(d, subj); q <- DD$q; om_dim <- switch(shape, q1 = 1L, diag = 2L, block = 3L)
  ols <- solve(DD$m * crossprod(DD$X), t(DD$X) %*% colSums(DD$Y)); th0 <- as.vector(ols)
  unpack_log <- function(x) {
    Om <- switch(shape, q1 = matrix(exp(x[3]), 1, 1), diag = diag(exp(x[3:4])),
                 block = { Lc <- matrix(c(x[3], x[4], 0, x[5]), 2, 2); Lc %*% t(Lc) })
    list(th = x[1:2], Om = Om, s2 = exp(x[3 + om_dim]))
  }
  unpack_nat <- function(x) {
    Om <- switch(shape, q1 = matrix(x[3], 1, 1), diag = diag(x[3:4]), block = { Lc <- matrix(c(x[3], x[4], 0, x[5]), 2, 2); Lc %*% t(Lc) })
    list(th = x[1:2], Om = Om, s2 = x[3 + om_dim])
  }
  f_log <- function(x) { u <- unpack_log(x); neg2ll_dbl(DD, u$th[1], u$th[2], u$Om, u$s2) }
  f_nat <- function(x) { u <- unpack_nat(x); neg2ll_dbl(DD, u$th[1], u$th[2], u$Om, u$s2) }
  starts <- list(c(1, 0.1, 1), c(5, 1, 0.5), c(0.1, 0.01, 2), c(10, 3, 5), c(0.01, 0.001, 0.2))      # (omega scale, second omega, sigma^2)
  runs <- list(); best <- NULL
  for (k in seq_along(starts)) {
    s <- starts[[k]]
    # --- logs
    x0 <- switch(shape, q1 = c(th0, log(s[1]), log(s[3])), diag = c(th0, log(s[1]), log(s[2]), log(s[3])),
                 block = c(th0, sqrt(s[1]), 0.1 * sqrt(s[2]), sqrt(s[2]), log(s[3])))
    ctl <- list(reltol = 1e-15, maxit = 20000)
    o <- optim(x0, f_log, method = "BFGS", control = ctl)
    o <- optim(o$par, f_log, method = "Nelder-Mead", control = list(reltol = 1e-15, maxit = 40000))
    o <- optim(o$par, f_log, method = "BFGS", control = ctl)
    u <- unpack_log(o$par)
    runs[[length(runs) + 1L]] <- list(kind = "logs", start = k, ofv = o$value, theta = u$th, omega = u$Om, sigma2 = u$s2)
    if (is.null(best) || o$value < best$ofv) best <- runs[[length(runs)]]
    # --- natural, bounded
    if (shape != "block") {
      x0n <- switch(shape, q1 = c(th0, s[1], s[3]), diag = c(th0, s[1], s[2], s[3]))
      lo <- c(-Inf, -Inf, rep(0, om_dim), 1e-10)
      o2 <- optim(x0n, f_nat, method = "L-BFGS-B", lower = lo, control = list(maxit = 20000, factr = 1, pgtol = 0))
      o2 <- optim(o2$par, f_nat, method = "L-BFGS-B", lower = lo, control = list(maxit = 20000, factr = 1, pgtol = 0))
      u2 <- unpack_nat(o2$par)
      runs[[length(runs) + 1L]] <- list(kind = "natural", start = k, ofv = o2$value, theta = u2$th, omega = u2$Om, sigma2 = u2$s2)
      if (o2$value < best$ofv) best <- runs[[length(runs)]]
    }
  }
  # the spread of the runs that reached the best objective to within 1e-6, for the record
  near <- Filter(function(r) r$ofv - best$ofv < 1e-6, runs)
  spread <- list(n_runs = length(runs), n_near_best = length(near),
                 ofv_spread = max(vapply(runs, function(r) r$ofv, 0)) - best$ofv,
                 ofv_spread_near = max(vapply(near, function(r) r$ofv, 0)) - best$ofv,
                 theta_spread_near = max(vapply(near, function(r) max(abs(r$theta - best$theta)), 0)),
                 omega_spread_near = max(vapply(near, function(r) max(abs(r$omega - best$omega)), 0)),
                 sigma2_spread_near = max(vapply(near, function(r) abs(r$sigma2 - best$sigma2), 0)))
  list(best = best, spread = spread)
}

# ============================================================================================ one case, end to end
om_names <- function(shape) switch(shape, q1 = "omega_full[0][0]", diag = c("omega_full[0][0]", "omega_full[1][1]"),
  block = c("omega_full[0][0]", "omega_full[1][0]", "omega_full[1][1]"))
# the labels of the answer (POP-API-01 item 13), in the order of the parameter vector
engine_names <- function(shape) c("theta1", "theta2", om_names(shape), "xi.a")
om_entries <- function(shape, Om) switch(shape, q1 = Om[1, 1], diag = c(Om[1, 1], Om[2, 2]), block = c(Om[1, 1], Om[2, 1], Om[2, 2]))
est_vec_dbl <- function(shape, th, Om, sigma, ofv) c(th, om_entries(shape, Om), sigma, ofv)
group_of <- function(shape) c("theta", "theta", rep("omega", length(om_names(shape))), "sigma", "ofv")
rep_groups <- function(shape, a, b) {                      # worst absolute difference by group
  g <- group_of(shape); dd <- abs(a - b)
  vapply(c("theta", "omega", "sigma", "ofv"), function(k) if (any(is.na(dd[g == k]))) NA_real_ else max(dd[g == k]), 0)
}
chol_ok <- function(A) tryCatch({ L <- chol_mp(A); all(vapply(seq_along(L), function(i) is.finite(L[[i]][i]) && L[[i]][i] > 0, TRUE)) }, error = function(e) FALSE)
mat_diag_dbl <- function(A) vapply(seq_along(A), function(i) dbl(A[[i]][i]), 0)
mat_sd_dbl <- function(A, k = 1) sqrt(k * mat_diag_dbl(A))                       # sqrt(k * diag(A)): a standard error from a covariance matrix

# The three covariance kinds of POP-COV-01 in the natural parameters, from the exact derivatives (256 bits).
cov_kinds <- function(shape, DD, par, free) {
  sj <- subject_jets(shape, DD, par); tj <- total_jet(sj)
  R <- sub_matrix(hess_matrix(tj), free); Rinv <- inv_mp(R)
  p <- length(free); Sm <- mat_zero(p, p)
  for (j in sj) { g <- grad_vec(j)[free]; for (a in seq_len(p)) for (b in seq_len(p)) Sm[[a]][b] <- Sm[[a]][b] + g[a] * g[b] }
  Sinv <- tryCatch(inv_mp(Sm), error = function(e) NULL)
  sand <- mat_mul(mat_mul(Rinv, Sm), Rinv)
  list(r = mat_sd_dbl(Rinv, 2), s = if (is.null(Sinv)) NULL else mat_sd_dbl(Sinv, 4),
       sandwich = mat_sd_dbl(sand), rinv_diag = mat_diag_dbl(Rinv), r_pd = chol_ok(R))
}

process_design <- function(d) {
  cat("== ", d$id, "\n", sep = "")
  # ------------------------------------------------------------------ data
  found <- NULL
  for (k in seq_len(d$n_seeds) - 1L) {
    subj <- generate(d, d$base_seed + k); cl <- classify_dbl(d, subj)
    ok <- switch(d$want, interior = if (d$shape == "block") cl$block_interior else cl$diag_interior,
      boundary = if (d$shape == "block") !cl$block_interior && !cl$diag_interior else !cl$diag_interior)
    if (isTRUE(ok)) { found <- k; break }
  }
  if (is.null(found)) stop("no seed gives the wanted design for ", d$id)
  seed <- d$base_seed + found
  if (d$generator == "h38") {                                # the data of H38 are checked against the table of the spec
    stopifnot(identical(unlist(lapply(subj, function(s) s$y)), H38_TABLE))
  }
  shape <- d$shape; q <- length(d$z_powers); m <- d$m; n <- length(d$times); N <- m * n
  DD <- design_mp(d, subj); DDd <- design_dbl(d, subj)
  # ------------------------------------------------------------------ references 2 and 3
  r3 <- direct_max(shape, d, subj)
  r2 <- run_lme(shape, d, subj, TRUE); r2d <- run_lme(shape, d, subj, FALSE)
  # ------------------------------------------------------------------ reference 1
  cf <- if (shape %in% c("q1", "block")) closed_form_mp(shape, DD) else list(kind = "no_closed_form")
  beta <- ols_mp(DD)$beta
  fshape <- shape                                           # the layout of the parameter vector used for the derivatives
  method <- NULL; polish <- NULL; kind <- "interior"; active <- integer(0)
  if (shape == "q1") {
    sg <- sqrt(cf$s2); om <- cf$omega[[1]][1]
    par <- mvec_(list(beta[1], beta[2], om, sg)); method <- "closed_form"
    if (cf$kind == "boundary") { kind <- "boundary"; active <- 3L }
  } else if (shape == "block" && cf$kind == "interior") {
    par <- mvec_(list(beta[1], beta[2], cf$omega[[1]][1], cf$omega[[2]][1], cf$omega[[2]][2], sqrt(cf$s2))); method <- "closed_form"
  } else if (shape == "block") {                             # boundary of the cone, no closed form: rank one, Omega = l l'
    Ob <- r3$best$omega; ev <- eigen(Ob, symmetric = TRUE)
    if (ev$values[2] > 1e-4 * ev$values[1]) stop(d$id, ": the double-precision maximum is not of rank one")
    l <- sqrt(ev$values[1]) * ev$vectors[, 1]; if (l[1] < 0) l <- -l
    pr <- mvec_(list(beta[1], beta[2], Mstr(l[1]), Mstr(l[2]), Mstr(sqrt(r3$best$sigma2))))
    pol <- newton_polish("rank1", DD, pr, 1:5)
    polish <- pol; l1 <- pol$par[3]; l2 <- pol$par[4]
    par <- mvec_(list(pol$par[1], pol$par[2], l1 * l1, l1 * l2, l2 * l2, pol$par[5])); method <- "newton_rank_one"
    kind <- "boundary"; fshape <- "block"
  } else {                                                   # diag
    Od <- diag(r3$best$omega); act <- Od < 1e-6
    par <- mvec_(list(beta[1], beta[2], if (act[1]) ZERO else Mstr(Od[1]), if (act[2]) ZERO else Mstr(Od[2]), Mstr(sqrt(r3$best$sigma2))))
    free <- setdiff(1:5, 2L + which(act))
    pol <- newton_polish("diag", DD, par, free); polish <- pol; par <- pol$par; method <- "newton"
    if (any(act)) { kind <- "boundary"; active <- 2L + which(act) }
  }
  p <- length(par)
  free <- setdiff(seq_len(p), active)
  if (fshape == "block" && method == "newton_rank_one") free <- c(1L, 2L, 6L)       # Omega is on the boundary of the cone: NC
  # the objective and the derivatives at the reference point
  tj <- total_jet(subject_jets(fshape, DD, par))
  g <- grad_vec(tj)
  stationarity <- if (method == "newton_rank_one") dbl(max(abs(g[c(1L, 2L, 6L)]))) else dbl(max(abs(g[free])))
  ofv_full <- tj$v; ofv_no2pi <- ofv_full - N * LN_2PI
  ofv_cf <- if (!is.null(cf$ofv)) cf$ofv else NULL
  kkt <- NULL
  if (kind == "boundary" && method != "newton_rank_one") kkt <- lapply(active, function(k) list(parameter = engine_names(fshape)[k], gradient = dbl(g[k])))
  if (method == "newton_rank_one") {                         # G = d OFV / d Omega at the rank-one point: G l = 0 and G positive semidefinite
    G <- list(mvec_(list(g[3], g[4] / 2)), mvec_(list(g[4] / 2, g[5])))
    l1 <- sqrt(par[3]); l2 <- par[4] / l1
    kkt <- list(list(parameter = "G l (first, second component)", gradient = c(dbl(G[[1]][1] * l1 + G[[1]][2] * l2), dbl(G[[2]][1] * l1 + G[[2]][2] * l2))),
                list(parameter = "other eigenvalue of dOFV/dOmega (trace)", gradient = dbl(G[[1]][1] + G[[2]][2])))
  }
  # standard errors (recorded, not asserted) and the curvature basis of the tolerances
  ck <- cov_kinds(fshape, DD, par, free)
  full_basis <- list(basis = "free_block", rinv = ck$rinv_diag, names = engine_names(fshape)[free])
  Rfull <- sub_matrix(hess_matrix(tj), seq_len(p))
  if (chol_ok(Rfull)) full_basis <- list(basis = "all_parameters", rinv = mat_diag_dbl(inv_mp(Rfull)), names = engine_names(fshape))
  # ------------------------------------------------------------------ comparison of the three references
  th_e <- c(dbl(par[1]), dbl(par[2])); sg_e <- dbl(par[p])
  Om_e <- switch(fshape, q1 = matrix(dbl(par[3]), 1, 1), diag = diag(c(dbl(par[3]), dbl(par[4]))),
                 block = matrix(c(dbl(par[3]), dbl(par[4]), dbl(par[4]), dbl(par[5])), 2, 2))
  e1 <- est_vec_dbl(shape, th_e, Om_e, sg_e, dbl(ofv_full))
  e3 <- est_vec_dbl(shape, r3$best$theta, r3$best$omega, sqrt(r3$best$sigma2), r3$best$ofv)
  as_vec <- function(r) if (isTRUE(r$ok)) est_vec_dbl(shape, r$theta, r$omega, r$sigma, r$ofv) else rep(NA_real_, length(e1))
  e2 <- as_vec(r2); e2d <- as_vec(r2d)
  disc <- list(nlme_tight = rep_groups(shape, e2, e1), nlme_default = rep_groups(shape, e2d, e1),
               direct = rep_groups(shape, e3, e1), nlme_vs_direct = rep_groups(shape, e2, e3))
  worst <- vapply(c("theta", "omega", "sigma", "ofv"), function(k) {
    v <- c(disc$nlme_tight[[k]], disc$direct[[k]], disc$nlme_vs_direct[[k]]); if (all(is.na(v))) NA_real_ else max(v, na.rm = TRUE) }, 0)
  signed <- function(a) as.list(setNames(a - e1, c(engine_names(shape), "ofv")))
  # ------------------------------------------------------------------ the file
  Om_str <- switch(fshape, q1 = list(list(S(par[3]))),
    diag = list(list(S(par[3]), "0"), list("0", S(par[4]))),
    block = list(list(S(par[3]), S(par[4])), list(S(par[4]), S(par[5]))))
  enames <- engine_names(shape)
  keep_se <- function(v) if (is.null(v)) NA else arr(v)
  nz <- function(x) if (is.null(x)) NA else x
  se_names <- {
    nm <- switch(fshape, q1 = c("theta1", "theta2", "omega_full[0][0]", "xi.a"), diag = c("theta1", "theta2", "omega_full[0][0]", "omega_full[1][1]", "xi.a"),
                 block = c("theta1", "theta2", "omega_full[0][0]", "omega_full[1][0]", "omega_full[1][1]", "xi.a"))
    nm[free]
  }
  mk_model <- function(st) list(name = "identity_linear", random_effects = if (q == 1L) arr("intercept") else arr(c("intercept", "slope")),
    theta = list(theta1 = st$theta[1], theta2 = st$theta[2]), omega = lapply(st$omega, arr), error_model = "additive", xi = list(a = st$sigma),
    z_powers = arr(d$z_powers))
  list(
    schema = 1L, kind = "pop_fit_exact", id = d$id, family = "d_fit_exact", description = d$description,
    precision_bits = BITS, digits = SIG,
    design = list(shape = shape, omega_shape = if (shape == "block") "block" else "diagonal", m = m, n_per_subject = n, n_observations = N,
                  times = arr(d$times), z_powers = arr(d$z_powers), generator = list(seed = seed, base_seed = d$base_seed, draws_skipped = found,
                  rule = "set.seed(seed, kind = \"Mersenne-Twister\", normal.kind = \"Inversion\", sample.kind = \"Rejection\"); per subject: the random effects, then rnorm(n, 0, sigma); y = theta1 + theta2 t + Z eta + error, rounded to 3 decimals",
                  truth = list(theta = arr(d$truth$theta), omega = lapply(d$truth$omega, arr), sigma = d$truth$sigma))),
    model = mk_model(list(theta = d$truth$theta, omega = d$truth$omega, sigma = d$truth$sigma)),
    starts = list(list(name = "generating_values", model = mk_model(list(theta = d$truth$theta, omega = d$truth$omega, sigma = d$truth$sigma))),
                  list(name = "far", model = mk_model(d$far))),
    subjects = lapply(subj, function(s) list(id = s$id, dose = s$dose, time = arr(s$time), y = arr(s$y))),
    expected = list(
      kind = kind, method = method, ofv_convention_of_values = "full",
      theta1 = S(par[1]), theta2 = S(par[2]), omega = Om_str, sigma = S(par[p]), sigma2 = S(par[p] * par[p]),
      ofv_full = S(ofv_full), ofv_no_2pi = S(ofv_no2pi), n_ln_2pi = S(N * LN_2PI),
      at_bound = arr(if (kind == "boundary") (if (method == "newton_rank_one") "omega_full (rank one)" else engine_names(fshape)[active]) else character()),
      sign_conditions = if (is.null(kkt)) list() else kkt),
    exact_reference = list(
      method = method, polish_iterations = if (is.null(polish)) NA else polish$iterations,
      polish_last_step = if (is.null(polish)) NA else polish$last_step,
      stationarity_gradient_max = stationarity,
      objective_minus_closed_form = if (is.null(ofv_cf)) NA else dbl(ofv_full - ofv_cf),
      closed_form_applies = cf$kind %in% c("interior", "boundary"),
      closed_form_kind = cf$kind),
    tolerance_basis = list(basis = full_basis$basis, parameters = arr(full_basis$names), rinv_diag = arr(full_basis$rinv),
      note = "diagonal of the inverse of R = the Hessian of the OFV (natural parameters, 256 bits): the OFV excess of an error d is d' R d / 2, so |d_j| <= sqrt(2 * eps * rinv_j) for an OFV excess of at most eps"),
    standard_errors = list(parameters = arr(se_names), r = keep_se(ck$r), s = keep_se(ck$s), sandwich = keep_se(ck$sandwich),
      note = "recorded, not asserted; the standard errors (square roots of the diagonals) of the covariance matrices of the natural parameters (variances, covariances, xi.a = sigma); r = 2 R^-1, s = 4 S^-1, sandwich = R^-1 S R^-1 (POP-COV-01); on a boundary the free block, conditional on the bound (POP-COV-03)",
      r_positive_definite = ck$r_pd),
    references = list(
      exact = list(method = method),
      nlme = list(tight = if (isTRUE(r2$ok)) list(theta = arr(r2$theta), omega = lapply(seq_len(nrow(r2$omega)), function(i) arr(r2$omega[i, ])), sigma = r2$sigma, ofv = r2$ofv) else list(failed = r2$message),
                  default = if (isTRUE(r2d$ok)) list(theta = arr(r2d$theta), omega = lapply(seq_len(nrow(r2d$omega)), function(i) arr(r2d$omega[i, ])), sigma = r2d$sigma, ofv = r2d$ofv) else list(failed = r2d$message)),
      direct = list(best_parameterisation = r3$best$kind, theta = arr(r3$best$theta), omega = lapply(seq_len(nrow(r3$best$omega)), function(i) arr(r3$best$omega[i, ])),
                    sigma2 = r3$best$sigma2, ofv = r3$best$ofv, runs = r3$spread),
      signed_difference_from_exact = list(names = arr(c(enames, "ofv")), nlme_tight = signed(e2), nlme_default = signed(e2d), direct = signed(e3)),
      worst_disagreement = list(note = "worst absolute difference by group among nlme (tight), the direct maximisation and each other; the exact point is reference 1",
        nlme_tight_vs_exact = as.list(disc$nlme_tight), nlme_default_vs_exact = as.list(disc$nlme_default),
        direct_vs_exact = as.list(disc$direct), nlme_tight_vs_direct = as.list(disc$nlme_vs_direct), worst_of_the_three = as.list(worst))))
}

# ============================================================================================ H38, checked before any fit
# The 30 rounded values of the table "The data of H38" in specs/sources.md, read from the file (rows S1 to S6, columns
# t = 0.5, 1, 2, 4, 8). The generator below must rebuild them exactly, as decimal strings, before anything is fitted.
H38_TABLE <- local({
  txt <- readLines(file.path(ROOT, "specs", "sources.md"), encoding = "UTF-8", warn = FALSE)
  at <- grep("The data of H38", txt, fixed = TRUE)[1]
  rows <- grep("^\\| S[1-6] \\|", txt[at:length(txt)], value = TRUE)[1:6]
  stopifnot(!anyNA(rows))
  unlist(lapply(rows, function(r) trimws(strsplit(r, "|", fixed = TRUE)[[1]][3:7])))
})
stopifnot(length(H38_TABLE) == 30L)
local({
  d <- DESIGNS[[1]]; stopifnot(d$id == "fit_q1_h38")
  got <- unlist(lapply(generate(d, d$base_seed), function(s) s$y))
  if (!identical(got, H38_TABLE)) stop("the generator of H38 does not rebuild the table of specs/sources.md")
  cat("H38: the 30 values rebuilt from the seed equal the table of specs/sources.md\n")
})

# ============================================================================================ run and write
for (d in DESIGNS) {
  res <- process_design(d)
  res$versions <- versions_list("nlme")
  write_json_lf(res, file.path(OUT, paste0(d$id, ".json")), digits = 17)
  w <- res$references$worst_disagreement$worst_of_the_three
  cat(sprintf("   %s [%s, %s]: worst of the three references: theta %.2e omega %.2e sigma %.2e ofv %.2e; stationarity %.2e\n",
              d$id, res$expected$kind, res$expected$method, w$theta, w$omega, w$sigma, w$ofv, res$exact_reference$stationarity_gradient_max))
}
files <- sort(list.files(OUT, pattern = "\\.json$", full.names = TRUE), method = "radix")
cat("sha256 of the", length(files), "files:\n")
for (f in files) cat(" ", digest::digest(file = f, algo = "sha256", serialize = FALSE), basename(f), "\n")
