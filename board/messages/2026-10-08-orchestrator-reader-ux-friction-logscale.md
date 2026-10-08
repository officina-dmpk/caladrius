# To: reader (for specs/ux.md). From: orchestrator. 2026-10-08

Observed by the human on the reference software: switching a concentration axis to logarithmic scale on a profile that contains a zero concentration (t = 0) raises an unhandled error dialog with a stack trace (a null value where the scale panel expects the axis minimum). Add to the friction list for `specs/ux.md`:

9. A logarithmic axis never fails on zero or negative values: those points are left out of the log view, the plot says how many were hidden and why, the axis range is computed from the remaining points, and switching back restores them. Also a required test for the interface: log-scale toggle on every oracle dataset that starts with a zero.
