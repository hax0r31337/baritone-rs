// Ported from baritone src/main/java/baritone/utils/pathing/Favoring.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df
//
// `Favoring(IPlayerContext, IPath, CalculationContext)` needs `Avoidance.create(ctx)` (phase
// 4); until then `with_avoidances` takes the avoidances it would create.

use rustc_hash::FxHashMap;

use crate::api::pathing::calc::IPath;
use crate::api::utils::BetterBlockPos;
use crate::pathing::movement::CalculationContext;
use crate::utils::pathing::Avoidance;

#[derive(Clone, Debug, Default)]
pub struct Favoring {
    /// A `Long2DoubleOpenHashMap` with default value 1.
    favorings: FxHashMap<i64, f64>,
}

impl Favoring {
    /// `Favoring(IPlayerContext, IPath, CalculationContext)`, with `Avoidance.create(ctx)`
    /// passed in as `avoidances`.
    pub fn with_avoidances(
        previous: Option<&dyn IPath>,
        context: &CalculationContext,
        avoidances: &[Avoidance],
    ) -> Self {
        let mut favoring = Self::new(previous, context);
        for avoid in avoidances {
            avoid.apply_spherical(&mut favoring.favorings);
        }
        crate::api::utils::helper::log_debug(&format!(
            "Favoring size: {}",
            favoring.favorings.len()
        ));
        favoring
    }

    /// create one just from previous path, no mob avoidances
    pub fn new(previous: Option<&dyn IPath>, context: &CalculationContext) -> Self {
        let mut favorings = FxHashMap::default();
        let coeff = context.backtrack_cost_favoring_coefficient;
        if coeff != 1.0
            && let Some(previous) = previous
        {
            for pos in previous.positions() {
                favorings.insert(BetterBlockPos::long_hash_pos(*pos), coeff);
            }
        }
        Self { favorings }
    }

    pub fn is_empty(&self) -> bool {
        self.favorings.is_empty()
    }

    pub fn calculate(&self, hash: i64) -> f64 {
        self.favorings.get(&hash).copied().unwrap_or(1.0)
    }
}
