# User experience: workflow, friction list and requirements (first version)

Card T-017 (part 2). Written by the `reader` agent on 2026-10-08, in its own words. Sources: the friction list of `AGENTS.md` section 7, the orchestrator message of 2026-10-08 on the logarithmic axis and the locale-dependent import (`board/messages/2026-10-08-orchestrator-reader-ux-friction-logscale.md`), and the thirteen screenshots in `private/screenshots/` (cited as "screens" with the name of their group: nca-setup 1 to 5, plot-and-worksheet 6 to 9, locale-and-preferences 10 and 11, preferences 12 and 13). The reference software is not named here.

## 0. How to read this file

This file **describes**. It does not transcribe labels, menu texts, dialog wording or layouts of the reference software, and no screenshot, concentration value or number from the human's worksheets is reproduced (`AGENTS.md` sections 3 and 7). The interface agent works from this file and the theme tokens and never opens the screenshots.

**Status tags** (`specs/README.md`), read for a user-interface specification:
- `documented, untested`: observed on the screenshots, or stated in `AGENTS.md` or in the orchestrator's message (allowed sources), and not yet covered by a test of the interface;
- `assumed`: a design decision for Caladrius, to be tested by the interface agent;
- `confirmed by oracle` is not used here: the oracle covers numbers, not screens. A rule that has a numeric core cites the rule of `specs/nca.md`, `specs/models.md` or `specs/fit.md` that carries it.

**Rule ids.** `UX-<AREA>-<nn>`. Areas: WF (the reference workflow, observed), FR (friction and what replaces it), REQ (required improvements of `AGENTS.md` section 7), IMP (import), LOG (logarithmic axes), LZ (λz selection), ERR (errors and messages), SET (settings).

**Terms.** "Reference" is the reference software as seen on the screenshots. "Worksheet" is a table of typed columns. "Analysis" is an NCA or a fit with its inputs and results.

## 1. The reference workflow, step by step

### UX-WF-01 Objects and the project tree
The reference organises a project as a tree: a data folder with worksheets, folders for code, tables, rules for values below the quantification limit, documents and shortcuts, and a workflow folder that holds the analyses and plots. Every analysis and every plot is an object in this tree, named by a running number (screens plot-and-worksheet 6 to 9, nca-setup 1). A path of links above the working area shows where the object sits.
- Status: `documented, untested`
- Sources: screens nca-setup 1, plot-and-worksheet 6 to 9.

### UX-WF-02 From a worksheet to an analysis
The user selects the worksheet in the tree, opens its context menu and walks through nested submenus (a "send to" entry, then a group of tool families, then plotting or non-compartmental analysis or modelling and the tool). The chosen tool appears as a new object under the workflow folder, with no data yet linked to its inputs (screen plot-and-worksheet 9, nca-setup 1).
- Status: `documented, untested`
- Sources: screens plot-and-worksheet 9, nca-setup 1; `AGENTS.md` section 7 friction 1.

### UX-WF-03 The analysis object: setup, results, verification
An analysis has three top-level pages. The setup page lists eight sub-pages in a left column (data mapping, dosing, slope selection, slopes, partial areas, therapeutic response, units, parameter names), shows the selected sub-page in the main area, and keeps a second set of tabs at the bottom (options, user-defined parameters, rules, plots). What the user decides is spread over these places: which column plays which role (sort key, time, concentration, carried column) in a grid of role buttons; the model family (plasma, urine, drug effect); the dose type (extravascular, IV bolus, IV infusion), its unit and a normalisation; the AUC calculation method; a weighting; the imputation at the dose time for steady state; titles; page breaks; whether to disable curve stripping; additional parameters defined by formula; rules for the λz fit and acceptance criteria (screens nca-setup 1 to 5).
- Status: `documented, untested`
- Sources: screens nca-setup 1 to 5; `AGENTS.md` section 7 friction 2.

### UX-WF-04 What the setup lets the user choose (decision inventory)
- AUC method: four choices, a linear trapezoid with linear interpolation (shown as the selected default), a log trapezoid, linear-up/log-down, and a mixed linear/log. `specs/nca.md` section 2.2 and NCA-AUC-05 to 07 define them. This is evidence for question Q-008 item 1 (the default is the linear trapezoid).
- Weighting: four choices, user-defined, uniform (selected), 1/Y and 1/(Y·Y), shown on the NCA setup, next to the AUC method. It most plausibly weights the λz regression; `specs/nca.md` section 12 lists a weighted λz regression as not specified, so this is a new open item (UX-O-01).
- λz rules page: a maximum number of points and an earliest start time for the best-fit option, and an acceptance block (adjusted R², percent extrapolated AUC, span, number of samples, percent extrapolated AUC in a dosing interval), all blank in the screenshot: no acceptance threshold is applied unless the user types one. This agrees with PKNCA's behaviour (`specs/nca.md` NCA-LZ-12) and is evidence for Q-008 item 3.
- Dose: type, unit, normalisation (none by default) and a preview button; an imputation choice (Cmin, Ctau, Clast) for non-bolus steady-state data.
- Not seen on the screenshots: the rule for values below the quantification limit (it is a project object with its own editor), the Tmax exclusion in λz, the C0 handling.
- Status: `documented, untested`
- Sources: screens nca-setup 1 to 5.

### UX-WF-05 Results as a tree
The results page shows a left tree of text output and plots opened one at a time, and a right area for the selected item (screens plot-and-worksheet 6, 7). The plot object has its own results page with a tree of plot options (layout, axes, graphs, legend, reference lines, annotations) and three tabs per node (content, appearance, axis label), where line weights, colours, styles, fonts and axis titles are set one by one.
- Status: `documented, untested`
- Sources: screens plot-and-worksheet 6, 7; `AGENTS.md` section 7 friction 3.

### UX-WF-06 The worksheet
A worksheet is a grid with typed columns. Each column has a type, a unit (free text with a builder) and a display format given as a format string (screens plot-and-worksheet 8, 9). Rows can repeat a time (several records at the same time are accepted and the plot draws them as a vertical jump, screens plot-and-worksheet 6, 8). Typing a value that the column does not accept raises a modal dialog that names the column and the value and says it is not a number, and the entry is rejected (screen locale-and-preferences 10).
- Status: `documented, untested`
- Sources: screens plot-and-worksheet 6, 8, 9, locale-and-preferences 10.

### UX-WF-07 Preferences
A modal preferences window with a tree of about twenty sections, most of them for other products (import from a laboratory system, remote execution, other modelling tools, statistics). The sections seen: the layout of the dosing worksheet imported from a laboratory system (a table of some forty rows with a transfer checkbox, a default and a transferred column name and a unit), the plot defaults (panel and margin sizes in pixels, resolution in dpi, fonts) and saved settings per object type (set, clear, delete, import, export) (screens locale-and-preferences 11, preferences 12, 13).
- Status: `documented, untested`
- Sources: screens locale-and-preferences 11, preferences 12, 13.

## 2. Friction list

Each item states what the reference does (observed or documented), what Caladrius does instead, and how an interface test would check it. Items 1 to 8 are those of `AGENTS.md` section 7; items 9 and 10 come from the orchestrator's message of 2026-10-08; items 11 to 14 are the reader's additions from the screenshots, proposed to the orchestrator.

### UX-FR-01 Creating an analysis takes a four-level nested menu
- Reference: UX-WF-02.
- Caladrius: one "New analysis" button that is always visible, and a command palette (every command has a stable id, golden rule 4: `nca.run`, `fit.run`, `plot.xy`, `data.import`); the palette also offers "new analysis from this worksheet".
- Test: from an open worksheet, an NCA is created in at most two actions (a click and a confirmation, or a palette entry), and the same command runs from the CLI.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 friction 1; screens.

### UX-FR-02 The settings of one analysis are spread over a left list, bottom tabs and a setup/results switch
- Reference: UX-WF-03, UX-WF-04.
- Caladrius: one page read from top to bottom: data, model or route and dose, units, options (AUC method, weighting, λz rules), initial estimates for a fit, with the plot always visible. Advanced options are in a collapsible section on the same page, not a separate tab. The results appear on the same screen (UX-REQ-01).
- Test: every option of the command's parameter structure is reachable on that page without leaving it; the page is serialisable as the command's parameters.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 friction 2.

### UX-FR-03 Results are a flat tree of about twenty tables and six plots, opened one at a time
- Reference: UX-WF-05.
- Caladrius: a summary first (for NCA: the headline parameters with their flags, the profile plot with the λz line and the chosen points; for a fit: parameters with precision, the fit plot, residuals and goodness of fit), details on demand (every table of the engine's output, `specs/fit.md` FIT-OUT-01 to 11).
- Test: the summary is visible without a click after a run; each detail table is one click away and can be exported.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 friction 3.

### UX-FR-04 A poor fit is not flagged
- Reference: a CV% in the hundreds or an interval across zero looks like any other number (observed by the human; the screenshots show no flag).
- Caladrius: the flags of `specs/fit.md` FIT-FLG-01 and `specs/nca.md` NCA-LZ-12b/12c are shown next to the number they concern, with the text that says what to check; thresholds are options. A flag never changes a number.
- Test: a fit with a parameter CV% above the threshold shows the flag in the summary; the replaced-value flag of `specs/nca.md` appears on the profile of case `edge_blq_set`.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 friction 4.

### UX-FR-05 Units are free text and inconsistent combinations pass silently
- Reference: units are typed as text per column (UX-WF-06), the dose unit has a separate builder.
- Caladrius: dose, concentration and time units are checked together before a run (`specs/nca.md` NCA-UNIT-02): a mass dose with a molar concentration asks for a molecular weight; a time unit different between data and infusion duration is an error; derived units are shown with every result. Units are data attached to the columns, never read from the locale.
- Test: dose in mg with concentration in nmol/L shows the warning and the offer before the run; results carry units.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 friction 5.

### UX-FR-06 Models are picked from a numbered table
- Reference: the model family is a list of numbered entries (the plasma family is labelled by a range of model numbers, screens nca-setup 1 to 3); for fitting the user picks a model by number.
- Caladrius: choose by route and number of compartments; the model id of `specs/models.md` MOD-GEN-05 (`pk1.oral_1_lag`, …) appears with its diagram and equation, and the parameters it needs.
- Test: choosing "oral, first order, with lag" selects `pk1.oral_1_lag` and shows the equation of MOD-AB1-05.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 friction 6; screens nca-setup 1 to 3.

### UX-FR-07 The project tree fills with objects named by running numbers
- Reference: UX-WF-01, objects are named like the tool and a counter.
- Caladrius: objects are named from their content (for example "NCA, oral 100 mg, subjects 1 to 12"), and an analysis whose input changed after its last run shows its results as stale, clearly.
- Test: editing a worksheet cell marks every dependent analysis stale; a new analysis gets a content-based name.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 friction 7.

### UX-FR-08 User models need a fixed-format block language (later step)
- Caladrius: readable equations with named parameters and immediate error messages, in the step after one-compartment models.
- Status: `assumed`
- Sources: `AGENTS.md` section 7 friction 8.

### UX-FR-09 A logarithmic axis fails on zero or negative values
- Reference (observed by the human, orchestrator's message): switching a concentration axis to a logarithmic scale on a profile that contains a zero concentration (time 0) raises an unhandled error dialog with a stack trace, a null value where the scale panel expects the axis minimum.
- Caladrius: UX-LOG-01 to UX-LOG-04.
- Status: `documented, untested`
- Sources: orchestrator message 2026-10-08, item 9.

### UX-FR-10 Locale-dependent import
- Reference (observed by the human, orchestrator's message): a CSV with decimal commas was imported with the comma taken as the column separator, so a value of a quarter became two cells, and the user noticed on the plot only; the course tells students to switch the operating system's region setting. The screenshots add a related failure: typing a decimal number with a point into a numeric cell under a comma locale is rejected with a modal dialog, the entry lost (screen locale-and-preferences 10).
- Caladrius: UX-IMP-01 to UX-IMP-06.
- Status: `documented, untested`
- Sources: orchestrator message 2026-10-08, item 10; screen locale-and-preferences 10.

### UX-FR-11 Duplicated times are accepted silently
- Reference: a worksheet may hold several rows with the same time; the analysis proceeds and the plot draws a vertical jump (UX-WF-06). In NCA this is a data error (`specs/nca.md` NCA-DAT-02: duplicated times are an error, never merged silently).
- Caladrius: the import preview and the data page flag rows that share a time within a profile, name them, and offer the explicit actions (keep the first, average, treat as separate profiles); nothing is merged without the user's choice.
- Test: a profile with two rows at time 0 shows the flag and the choices before any run.
- Status: `assumed`
- Sources: screens plot-and-worksheet 6, 8; `specs/nca.md` NCA-DAT-02. Proposed by the reader.

### UX-FR-12 Acceptance criteria are hidden and blank
- Reference: acceptance thresholds for the λz fit sit in a rules tab of the bottom tabs and are blank by default (UX-WF-04); the result carries no sign that a criterion was or was not applied.
- Caladrius: the same thresholds are the quality flags of `specs/nca.md` NCA-LZ-12b with visible defaults (adjusted R² 0.9, span ratio 2, extrapolated AUC 20 %, 3 points) on the analysis page, switchable off, and the result says which were applied.
- Test: the flag thresholds are visible on the page; turning one off removes the flag and nothing else.
- Status: `assumed`
- Sources: screen nca-setup 5; `specs/nca.md` NCA-LZ-12, NCA-LZ-12b. Proposed by the reader.

### UX-FR-13 Plot options are a tree with three tabs per node
- Reference: UX-WF-05; line weights, colours, fonts and titles are set node by node; plot sizes in the defaults are in pixels.
- Caladrius: a plot is styled by a theme token file (light and dark, `AGENTS.md` golden rule 7) with a few visible controls (scale, title, series visibility); advanced styling is data in the plot's serialisable state, not a deep tree.
- Test: the plot's state round-trips as data; switching theme changes no hard-coded colour.
- Status: `assumed`
- Sources: screens plot-and-worksheet 6, 7, preferences 12. Proposed by the reader.

### UX-FR-14 Settings are in a modal tree dominated by other tools
- Reference: UX-WF-07.
- Caladrius: a settings page with only what the application uses (language, number and date display, default options of commands, plot theme), searchable, each default showing its value and what it affects.
- Test: every setting is a field of a serialisable settings structure; no setting is read from the operating-system locale without being shown.
- Status: `assumed`
- Sources: screens locale-and-preferences 11, preferences 12, 13. Proposed by the reader.

## 3. Required improvements (`AGENTS.md` section 7)

### UX-REQ-01 One screen from data to results for NCA
Data (table and plot), route and dose, options, and the NCA result with its profile plot are on one screen; changing an option re-runs the engine immediately (the NCA is fast) and updates the result and the flags. The worksheet of the source data stays editable on the same screen.
- Status: `assumed`
- Sources: `AGENTS.md` section 7; UX-FR-02, UX-FR-03.

### UX-REQ-02 Live curve while the initial estimates change
For a fit, the predicted curve of `specs/models.md` is drawn over the data and updates as each initial estimate is edited (spinner, slider or typed value); the generated initial estimates of `specs/fit.md` FIT-INI-01 to 04 are the starting values and can be edited; the objective (WRSS) is shown.
- Status: `assumed`
- Sources: `AGENTS.md` section 7; `specs/fit.md`.

### UX-REQ-03 Every error says what to fix
An error message names the field, the value found and the action that fixes it (`AGENTS.md` golden rule 6). The engine's errors already carry the parameter or time; the interface adds where to click.
- Status: `assumed`
- Sources: `AGENTS.md` sections 3 and 7.

## 4. λz selection

### UX-LZ-01 Click points on the curve to choose the λz range
The profile plot shows, on a semi-log view, every eligible point (`specs/nca.md` NCA-LZ-02, NCA-LZ-02c) and the points used by the current fit; the user clicks a point to include or exclude it, or drags to select a time range; each change calls the engine with a manual selection (NCA-LZ-08) and updates λz, half-life, R², adjusted R², span ratio and the flags at once. The table of candidate fits (NCA-OUT-01) is shown beside the plot and a click on a row selects that candidate. A "back to automatic" action restores the automatic choice.
- Status: `assumed`
- Sources: `AGENTS.md` section 7; `specs/nca.md` NCA-OUT-01, NCA-LZ-08, NCA-LZ-12b.

### UX-LZ-02 Replaced values are shown as such
Points whose value was replaced by the BLQ or missing-value policy (`specs/nca.md` NCA-DAT-07b) are drawn with a distinct marker and a tooltip "replaced value"; if the fit uses some, the flag `replaced_points_in_lambda_z` is shown; the option to exclude them (NCA-LZ-02d) is one click away.
- Status: `assumed`
- Sources: `specs/nca.md` NCA-DAT-07b, NCA-LZ-02b, NCA-LZ-02d, NCA-LZ-12c.

## 5. Logarithmic axes

### UX-LOG-01 A logarithmic axis never fails
Switching an axis to a logarithmic scale never raises an error or leaves the axis without a range. Points that cannot be drawn on a log axis (a concentration or time equal to 0, or negative) are left out of the logarithmic view only; no value of the data is changed.
- Status: `documented, untested`
- Sources: orchestrator message 2026-10-08, item 9.

### UX-LOG-02 The plot says what is hidden and why
A visible, non-modal note on the plot gives the number of hidden points and the reason, for example "2 of 9 points not shown on a log axis (zero or negative values)"; a click on the note lists the hidden points (time and value) and offers to switch back.
- Status: `documented, untested`
- Sources: orchestrator message 2026-10-08, item 9. Worked example below.

### UX-LOG-03 The range comes from the remaining points
The axis limits are computed from the points that remain, rounded outward to whole decades for the tick labels (lower limit the decade at or below the smallest remaining value, upper limit the decade at or above the largest); with a single remaining point the range is one decade around it. If no point remains (all zero or negative), the plot shows an empty state with the message of UX-LOG-02 instead of an axis.
- Status: `assumed`
- Sources: orchestrator message 2026-10-08, item 9; design choice for the rounding.

### UX-LOG-04 Switching back restores everything
Returning to the linear scale shows every point again, with the previous range; the toggle can be repeated any number of times without changing the data or the analysis results. A logarithmic time axis follows the same rules for a time of 0.
- Status: `documented, untested`
- Sources: orchestrator message 2026-10-08, item 9.

### UX-LOG-05 Required interface test
For every oracle dataset whose profiles start with a zero concentration, the interface test toggles the concentration axis to logarithmic and back and checks: no error, the count of hidden points equals the number of non-positive values, the range comes from the positive ones, and the restored linear view equals the original. The datasets in `oracle/data/` with a first concentration of 0 are: `theoph` (subjects 2 to 6, 8, 9, 11 and 12), `synthetic_lz` (all four), `edge_blq` (all six), `edge_missing` (all five, with empty values also left out), `edge_negative` (both, with negative values), `edge_oral` (subjects 1, 2, 3, 6), `edge_iv` (subjects 2 and 3), `edge_infusion` (all three). `indometh` has no zero and checks the case where nothing is hidden. The fit datasets are the same profiles.
- Status: `assumed`
- Sources: orchestrator message 2026-10-08, item 9 (the test is required there); `oracle/data/*.csv` (read by the reader).

**Worked example (UX-LOG-02, UX-LOG-03).** Profile 1 of `edge_blq` (time 0, 0.5, 1, 2, 4, 6, 8, 12, 24 h; concentrations 0, 2.1, 6.3, 0, 4.4, 3.1, 2.2, 1.1, 0.52): two points are zero (times 0 and 2), so seven of nine are shown and the note reads "2 of 9 points not shown on a log axis (zero or negative values)". The remaining values run from 0.52 to 6.3, so the axis is drawn from 0.1 to 10 (decades below 0.52 and above 6.3). Switching back shows nine points on the linear axis. A profile in which every concentration is zero shows the empty state and the note "9 of 9 points not shown", with no axis.

## 6. Import of tabular data

### UX-IMP-01 Detect the separator and the decimal convention
When a file is opened, Caladrius reads a sample (the first rows) and determines the field separator (comma, semicolon, tab, vertical bar or runs of spaces) and the decimal mark (point or comma) by consistency, not by locale: a candidate pair is admissible when every row has the same number of fields as the header and every cell of a column that looks numeric parses as a number under it. If a separator that is not the comma gives consistent rows, and commas occur only inside fields that parse as decimals with one comma, the comma is the decimal mark (a semicolon file with "0,25" is a decimal comma). If exactly one admissible pair exists it is proposed; if several do, the preview shows the alternatives and the user chooses; if none, the file is refused with the reasons (UX-IMP-04).
- Status: `assumed`
- Sources: orchestrator message 2026-10-08, item 10 (sniff the file, let the user confirm in a preview); design choice for the algorithm.

### UX-IMP-02 Preview before anything runs
The parsed table is shown with its row and column counts, the detected separator and decimal mark (editable), the detected units if the header carries them, and the first and last rows, before the data enters the project. Nothing is imported and no analysis is run until the user confirms. The preview is the same screen for a pasted table.
- Status: `assumed`
- Sources: orchestrator message 2026-10-08, item 10.

### UX-IMP-03 Never split or lose a numeric cell
An import is refused, not repaired, when a numeric cell would be split into two cells, merged with another, truncated, or lost: a row with more fields than the header, a numeric column with a non-numeric cell (other than an explicit missing-value marker), a decimal mark that differs between cells of the same column, a thousands separator mixed with a decimal comma, or an unterminated quoted field.
- Status: `assumed`
- Sources: orchestrator message 2026-10-08, item 10.

### UX-IMP-04 The refusal says what to fix
The message names the first offending row and column, shows the cell as read, and offers the way out: change the separator, change the decimal mark, or fix the file (for example "row 4, column 2: '25,5' contains a comma but the decimal mark is a point; choose decimal comma or edit the file").
- Status: `assumed`
- Sources: `AGENTS.md` section 7 (every error says what to fix); orchestrator message item 10.

### UX-IMP-05 Units and display precision are data
The unit of a column is a field of the data (read from the header if present, or chosen by the user), never inferred from the locale; the number of decimals shown is a display setting of the column and never rounds the stored value. Typing a number into a cell accepts both marks according to the shown convention and says which it understood; an entry that cannot be read is kept in the cell and marked, not discarded.
- Status: `assumed`
- Sources: orchestrator message 2026-10-08, item 10; screen locale-and-preferences 10 (the failure it replaces).

### UX-IMP-06 Required tests
(a) A semicolon file with decimal commas and a comma file with decimal points, holding the same profile, give identical tables. (b) A file in which the same text "0,25" could be read two ways is shown with both readings and not imported until the user picks one. (c) A row with an extra field is refused with the row number. (d) The oracle datasets, exported by the test in three conventions (comma and point, semicolon and comma, tab and point) and re-imported, return the oracle values exactly (to the 15 digits written in `oracle/data/*.csv`).
- Status: `assumed`
- Sources: design; orchestrator message item 10.

**Worked examples (UX-IMP-01).** (1) Text `time;conc` then `0,25;3,4`: the semicolon gives two fields in both rows; commas occur inside fields that parse as decimals with one comma; the pair (semicolon, decimal comma) is the only admissible one and yields time 0.25, concentration 3.4. (2) Text `time,conc` then rows such as `0,25`, `0,5`, `1,2`: with the comma as separator every row has two fields like the header, so by consistency alone that reading is admissible, and it is exactly the silent failure reported by the human (a quarter read as time 0 and concentration 25), because the author meant a decimal comma in a one-column-per-row layout, or a semicolon was lost on the way. The reading is therefore subjected to plausibility checks on the result: within a profile the time column must be non-decreasing and must have as many distinct values as rows (here it would be 0, 0, 1 and repeats zeros), and no column may consist only of the digits that follow a mark. A reading that fails a check is never imported silently: the preview shows it next to the alternative reading and asks for a choice. The checks are a heuristic (UX-O-02).

## 7. Settings

### UX-SET-01 Locale is displayed, not assumed
The interface language and the number display follow the user's choice, with the operating-system locale only as the initial suggestion, shown in the settings. Changing the locale never changes stored data or what a file import understands (UX-IMP-01).
- Status: `assumed`
- Sources: orchestrator message item 10 (the course tells students to switch the system region); screen locale-and-preferences 10.

## 8. Open items

| id | item | rule | who and how |
|---|---|---|---|
| UX-O-01 | The reference's NCA setup offers a weighting (uniform, 1/Y, 1/(Y·Y), user-defined); `specs/nca.md` does not specify a weighted λz regression. Does it weight the λz regression, and how? | UX-WF-04 | the human's screen or the private oracle; a new item for `specs/nca.md` |
| UX-O-02 | The ambiguity heuristic of UX-IMP-01 (a file whose only admissible reading is the wrong one) needs real examples; the reader has only the one reported by the human | UX-IMP-01 | the human, with a sample file (no data from `private/` in a versioned file) |
| UX-O-03 | Which of the proposed friction items 11 to 14 the orchestrator adopts | UX-FR-11 to 14 | orchestrator |
| UX-O-04 | The reference's rule for values below the quantification limit is a project object whose editor was not screenshotted; Q-008 item 2 stays open | UX-WF-04 | the human's screen |
| UX-O-05 | Workflows not seen: fitting (model setup, initial estimates, results of a fit), partial areas, therapeutic response, tables; only the NCA setup, the plot object, the worksheet and the preferences were screenshotted | all | new screenshots, then a second version of this file |
