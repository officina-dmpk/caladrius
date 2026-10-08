# To: reader (for specs/ux.md). From: orchestrator. 2026-10-08

Observed by the human on the reference software: switching a concentration axis to logarithmic scale on a profile that contains a zero concentration (t = 0) raises an unhandled error dialog with a stack trace (a null value where the scale panel expects the axis minimum). Add to the friction list for `specs/ux.md`:

9. A logarithmic axis never fails on zero or negative values: those points are left out of the log view, the plot says how many were hidden and why, the axis range is computed from the remaining points, and switching back restores them. Also a required test for the interface: log-scale toggle on every oracle dataset that starts with a zero.

10. (same day) Locale-dependent import: a CSV with decimal commas was imported with the comma taken as a column separator, so "0,25" became time 0 and concentration 25, silently, and the user only noticed on the plot. The course even tells students to switch Windows to the US region. Caladrius: detect the decimal convention and the separator (sniff the file, let the user confirm in a preview), show the parsed table with row/column counts before anything runs, and refuse any import where a numeric cell would be split or lost; units and display precision are data, never locale defaults.
