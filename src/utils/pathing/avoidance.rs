// Ported from baritone src/main/java/baritone/utils/pathing/Avoidance.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

use rustc_hash::FxHashMap;

use crate::api::utils::{BetterBlockPos, IPlayerContext};
use crate::host::Entity;
use crate::settings::settings;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Avoidance {
    center_x: i32,
    center_y: i32,
    center_z: i32,
    coefficient: f64,
    radius: i32,
    radius_sq: i32,
}

impl Avoidance {
    /// `Avoidance(BlockPos, double, int)`
    pub fn from_pos(center: BetterBlockPos, coefficient: f64, radius: i32) -> Self {
        Self::new(center.x, center.y, center.z, coefficient, radius)
    }

    pub fn new(center_x: i32, center_y: i32, center_z: i32, coefficient: f64, radius: i32) -> Self {
        Self {
            center_x,
            center_y,
            center_z,
            coefficient,
            radius,
            radius_sq: radius.wrapping_mul(radius),
        }
    }

    pub fn coefficient(&self, x: i32, y: i32, z: i32) -> f64 {
        let x_diff = x.wrapping_sub(self.center_x);
        let y_diff = y.wrapping_sub(self.center_y);
        let z_diff = z.wrapping_sub(self.center_z);
        if x_diff
            .wrapping_mul(x_diff)
            .wrapping_add(y_diff.wrapping_mul(y_diff))
            .wrapping_add(z_diff.wrapping_mul(z_diff))
            <= self.radius_sq
        {
            self.coefficient
        } else {
            1.0
        }
    }

    /// `map` is a `Long2DoubleOpenHashMap` whose default value is 1.
    pub fn apply_spherical(&self, map: &mut FxHashMap<i64, f64>) {
        let radius = self.radius;
        for x in -radius..=radius {
            for y in -radius..=radius {
                for z in -radius..=radius {
                    if x.wrapping_mul(x)
                        .wrapping_add(y.wrapping_mul(y))
                        .wrapping_add(z.wrapping_mul(z))
                        <= radius.wrapping_mul(radius)
                    {
                        let hash = BetterBlockPos::long_hash(
                            self.center_x.wrapping_add(x),
                            self.center_y.wrapping_add(y),
                            self.center_z.wrapping_add(z),
                        );
                        let value = map.get(&hash).copied().unwrap_or(1.0) * self.coefficient;
                        map.insert(hash, value);
                    }
                }
            }
        }
    }

    /// `create(IPlayerContext)`. The mob spawners come from the chunk cache, which is not
    /// ported, so only mobs are avoided.
    pub fn create(ctx: &dyn IPlayerContext) -> Vec<Avoidance> {
        let settings = settings();
        if !settings.avoidance {
            return Vec::new();
        }
        let mut res = Vec::new();
        let mob_coeff = settings.mob_avoidance_coefficient;
        // mobSpawnerAvoidanceCoefficient: ctx.worldData().getCachedWorld().getLocationsOf("mob_spawner", ...)
        if mob_coeff != 1.0 {
            for entity in ctx.entities() {
                if !entity.mob {
                    continue;
                }
                if entity.is_spider() && ctx.player().light_level_dependent_magic_value >= 0.5 {
                    continue;
                }
                if entity.type_id == Entity::ZOMBIFIED_PIGLIN && !entity.provoked {
                    continue;
                }
                if entity.type_id == Entity::ENDERMAN && !entity.creepy {
                    continue;
                }
                res.push(Avoidance::from_pos(
                    entity.block_position(),
                    mob_coeff,
                    settings.mob_avoidance_radius,
                ));
            }
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spherical() {
        let a = Avoidance::new(10, 64, -3, 1.5, 2);
        let mut map = FxHashMap::default();
        a.apply_spherical(&mut map);
        // points with x²+y²+z² <= 4 in a 5x5x5 cube
        let mut expected = 0;
        for x in -2i32..=2 {
            for y in -2i32..=2 {
                for z in -2i32..=2 {
                    if x * x + y * y + z * z <= 4 {
                        expected += 1;
                    }
                }
            }
        }
        assert_eq!(map.len(), expected);
        assert_eq!(map[&BetterBlockPos::long_hash(10, 64, -3)], 1.5);
        a.apply_spherical(&mut map);
        assert_eq!(map[&BetterBlockPos::long_hash(12, 64, -3)], 2.25);
        assert!(!map.contains_key(&BetterBlockPos::long_hash(12, 65, -3)));

        assert_eq!(a.coefficient(11, 65, -3), 1.5);
        assert_eq!(a.coefficient(12, 65, -3), 1.0);
    }
}
