//! Model catalogue and stable ids (`specs/models.md` MOD-GEN-05, MOD-2C-20).

use serde::{Deserialize, Serialize};

/// Single-dose model: one compartment (`pk1.*`) or two compartments (`pk2.*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelId {
    /// `pk1.iv_bolus` (MOD-IVB-01): parameters `v` and `cl` or `k`.
    #[serde(rename = "pk1.iv_bolus")]
    IvBolus,
    /// `pk1.iv_infusion` (MOD-IVI-01): `v`, `cl` or `k`, `dur`.
    #[serde(rename = "pk1.iv_infusion")]
    IvInfusion,
    /// `pk1.oral_1` (MOD-AB1-01): `v`, `cl` or `k`, `ka`.
    #[serde(rename = "pk1.oral_1")]
    Oral1,
    /// `pk1.oral_1_lag` (MOD-AB1-05): `v`, `cl` or `k`, `ka`, `tlag`.
    #[serde(rename = "pk1.oral_1_lag")]
    Oral1Lag,
    /// `pk1.oral_0` (MOD-AB0-01): `v`, `cl` or `k`, `dur`.
    #[serde(rename = "pk1.oral_0")]
    Oral0,
    /// `pk1.oral_0_lag` (MOD-AB0-02): `v`, `cl` or `k`, `dur`, `tlag`.
    #[serde(rename = "pk1.oral_0_lag")]
    Oral0Lag,
    /// `pk2.iv_bolus` (MOD-2C-07): one parameter set of MOD-2C-02.
    #[serde(rename = "pk2.iv_bolus")]
    Pk2IvBolus,
    /// `pk2.iv_infusion` (MOD-2C-08): a parameter set and `dur`.
    #[serde(rename = "pk2.iv_infusion")]
    Pk2IvInfusion,
    /// `pk2.oral_1` (MOD-2C-09, 10): a parameter set and `ka`.
    #[serde(rename = "pk2.oral_1")]
    Pk2Oral1,
    /// `pk2.oral_1_lag` (MOD-2C-11): a parameter set, `ka` and `tlag`.
    #[serde(rename = "pk2.oral_1_lag")]
    Pk2Oral1Lag,
    /// `pk2.oral_0` (MOD-2C-12): a parameter set and `dur`.
    #[serde(rename = "pk2.oral_0")]
    Pk2Oral0,
    /// `pk2.oral_0_lag` (MOD-2C-12): a parameter set, `dur` and `tlag`.
    #[serde(rename = "pk2.oral_0_lag")]
    Pk2Oral0Lag,
}

/// How the drug enters the central compartment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Input {
    Bolus,
    ZeroOrder,
    FirstOrder,
}

impl ModelId {
    /// Every one-compartment model, in catalogue order (the catalogue the engine and the UI offer
    /// today; the two-compartment ids are [`ModelId::TWO_COMPARTMENT`]).
    pub const ALL: [ModelId; 6] = [
        ModelId::IvBolus,
        ModelId::IvInfusion,
        ModelId::Oral1,
        ModelId::Oral1Lag,
        ModelId::Oral0,
        ModelId::Oral0Lag,
    ];

    /// Every two-compartment model, in catalogue order (MOD-2C-20).
    pub const TWO_COMPARTMENT: [ModelId; 6] = [
        ModelId::Pk2IvBolus,
        ModelId::Pk2IvInfusion,
        ModelId::Pk2Oral1,
        ModelId::Pk2Oral1Lag,
        ModelId::Pk2Oral0,
        ModelId::Pk2Oral0Lag,
    ];

    /// The stable id, e.g. `pk1.oral_1`.
    pub fn id(&self) -> &'static str {
        match self {
            ModelId::IvBolus => "pk1.iv_bolus",
            ModelId::IvInfusion => "pk1.iv_infusion",
            ModelId::Oral1 => "pk1.oral_1",
            ModelId::Oral1Lag => "pk1.oral_1_lag",
            ModelId::Oral0 => "pk1.oral_0",
            ModelId::Oral0Lag => "pk1.oral_0_lag",
            ModelId::Pk2IvBolus => "pk2.iv_bolus",
            ModelId::Pk2IvInfusion => "pk2.iv_infusion",
            ModelId::Pk2Oral1 => "pk2.oral_1",
            ModelId::Pk2Oral1Lag => "pk2.oral_1_lag",
            ModelId::Pk2Oral0 => "pk2.oral_0",
            ModelId::Pk2Oral0Lag => "pk2.oral_0_lag",
        }
    }

    /// The model with this stable id (one or two compartments).
    pub fn from_id(id: &str) -> Option<ModelId> {
        Self::ALL
            .into_iter()
            .chain(Self::TWO_COMPARTMENT)
            .find(|m| m.id() == id)
    }

    /// Number of compartments of the disposition (1 or 2); data, not an assumption (golden rule 5).
    pub fn compartments(&self) -> u8 {
        if Self::TWO_COMPARTMENT.contains(self) {
            2
        } else {
            1
        }
    }

    pub(crate) fn input(&self) -> Input {
        match self {
            ModelId::IvBolus | ModelId::Pk2IvBolus => Input::Bolus,
            ModelId::IvInfusion
            | ModelId::Oral0
            | ModelId::Oral0Lag
            | ModelId::Pk2IvInfusion
            | ModelId::Pk2Oral0
            | ModelId::Pk2Oral0Lag => Input::ZeroOrder,
            ModelId::Oral1 | ModelId::Oral1Lag | ModelId::Pk2Oral1 | ModelId::Pk2Oral1Lag => {
                Input::FirstOrder
            }
        }
    }

    pub(crate) fn has_lag(&self) -> bool {
        matches!(
            self,
            ModelId::Oral1Lag | ModelId::Oral0Lag | ModelId::Pk2Oral1Lag | ModelId::Pk2Oral0Lag
        )
    }

    /// Parameter names this model accepts besides its disposition parameters (`v` and one of `cl`,
    /// `k` for one compartment, MOD-VOC-01; one set of MOD-2C-02 for two compartments).
    pub fn input_parameters(&self) -> &'static [&'static str] {
        match (self.input(), self.has_lag()) {
            (Input::Bolus, _) => &[],
            (Input::ZeroOrder, false) => &["dur"],
            (Input::ZeroOrder, true) => &["dur", "tlag"],
            (Input::FirstOrder, false) => &["ka"],
            (Input::FirstOrder, true) => &["ka", "tlag"],
        }
    }
}
