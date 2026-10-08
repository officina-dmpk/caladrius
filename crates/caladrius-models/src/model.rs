//! Model catalogue and stable ids (`specs/models.md` MOD-GEN-05).

use serde::{Deserialize, Serialize};

/// One-compartment, single-dose model.
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
}

/// How the drug enters the central compartment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Input {
    Bolus,
    ZeroOrder,
    FirstOrder,
}

impl ModelId {
    /// Every model, in catalogue order.
    pub const ALL: [ModelId; 6] = [
        ModelId::IvBolus,
        ModelId::IvInfusion,
        ModelId::Oral1,
        ModelId::Oral1Lag,
        ModelId::Oral0,
        ModelId::Oral0Lag,
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
        }
    }

    /// The model with this stable id.
    pub fn from_id(id: &str) -> Option<ModelId> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    pub(crate) fn input(&self) -> Input {
        match self {
            ModelId::IvBolus => Input::Bolus,
            ModelId::IvInfusion | ModelId::Oral0 | ModelId::Oral0Lag => Input::ZeroOrder,
            ModelId::Oral1 | ModelId::Oral1Lag => Input::FirstOrder,
        }
    }

    pub(crate) fn has_lag(&self) -> bool {
        matches!(self, ModelId::Oral1Lag | ModelId::Oral0Lag)
    }

    /// Parameter names this model accepts, besides `v` and one of `cl`, `k` (MOD-VOC-01).
    pub fn input_parameters(&self) -> &'static [&'static str] {
        match self {
            ModelId::IvBolus => &[],
            ModelId::IvInfusion | ModelId::Oral0 => &["dur"],
            ModelId::Oral0Lag => &["dur", "tlag"],
            ModelId::Oral1 => &["ka"],
            ModelId::Oral1Lag => &["ka", "tlag"],
        }
    }
}
