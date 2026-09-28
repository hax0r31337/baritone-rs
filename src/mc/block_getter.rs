// Ported from Minecraft 26.3 net/minecraft/world/level/BlockGetter.java (client jar bytecode)
//
// Only `traverseBlocks`, the grid walk under `clip`. `clip` itself is `crate::host::World::clip`,
// which reads the host's block shapes.

use crate::api::utils::BetterBlockPos;
use crate::mc::{Vec3, mth};

/// `traverseBlocks(Vec3, Vec3, C, BiFunction, Function)`: visits the blocks the segment
/// `from`-`to` passes through, in order, until `consumer` returns a result.
pub fn traverse_blocks<T>(
    from: Vec3,
    to: Vec3,
    mut consumer: impl FnMut(BetterBlockPos) -> Option<T>,
    miss_factory: impl FnOnce() -> T,
) -> T {
    if from.equals(to) {
        return miss_factory();
    }
    let to_x = mth::lerp(-1.0E-7, to.x, from.x);
    let to_y = mth::lerp(-1.0E-7, to.y, from.y);
    let to_z = mth::lerp(-1.0E-7, to.z, from.z);
    let from_x = mth::lerp(-1.0E-7, from.x, to.x);
    let from_y = mth::lerp(-1.0E-7, from.y, to.y);
    let from_z = mth::lerp(-1.0E-7, from.z, to.z);
    let mut current_block_x = mth::floor(from_x);
    let mut current_block_y = mth::floor(from_y);
    let mut current_block_z = mth::floor(from_z);
    if let Some(first) = consumer(BetterBlockPos::new(
        current_block_x,
        current_block_y,
        current_block_z,
    )) {
        return first;
    }
    let dx = to_x - from_x;
    let dy = to_y - from_y;
    let dz = to_z - from_z;
    let sign_x = mth::sign(dx);
    let sign_y = mth::sign(dy);
    let sign_z = mth::sign(dz);
    let t_delta_x = if sign_x == 0 {
        f64::MAX
    } else {
        sign_x as f64 / dx
    };
    let t_delta_y = if sign_y == 0 {
        f64::MAX
    } else {
        sign_y as f64 / dy
    };
    let t_delta_z = if sign_z == 0 {
        f64::MAX
    } else {
        sign_z as f64 / dz
    };
    let mut t_x = t_delta_x
        * if sign_x > 0 {
            1.0 - mth::frac(from_x)
        } else {
            mth::frac(from_x)
        };
    let mut t_y = t_delta_y
        * if sign_y > 0 {
            1.0 - mth::frac(from_y)
        } else {
            mth::frac(from_y)
        };
    let mut t_z = t_delta_z
        * if sign_z > 0 {
            1.0 - mth::frac(from_z)
        } else {
            mth::frac(from_z)
        };

    while t_x <= 1.0 || t_y <= 1.0 || t_z <= 1.0 {
        if t_x < t_y {
            if t_x < t_z {
                current_block_x = current_block_x.wrapping_add(sign_x);
                t_x += t_delta_x;
            } else {
                current_block_z = current_block_z.wrapping_add(sign_z);
                t_z += t_delta_z;
            }
        } else if t_y < t_z {
            current_block_y = current_block_y.wrapping_add(sign_y);
            t_y += t_delta_y;
        } else {
            current_block_z = current_block_z.wrapping_add(sign_z);
            t_z += t_delta_z;
        }

        if let Some(result) = consumer(BetterBlockPos::new(
            current_block_x,
            current_block_y,
            current_block_z,
        )) {
            return result;
        }
    }

    miss_factory()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_the_grid_in_order() {
        let mut visited = Vec::new();
        let missed = traverse_blocks(
            Vec3::new(0.5, 0.5, 0.5),
            Vec3::new(2.5, 1.5, 0.5),
            |pos| {
                visited.push(pos);
                None::<bool>
            },
            || false,
        );
        assert!(!missed);
        assert_eq!(
            visited,
            [
                BetterBlockPos::new(0, 0, 0),
                BetterBlockPos::new(1, 0, 0),
                BetterBlockPos::new(1, 1, 0),
                BetterBlockPos::new(2, 1, 0),
            ]
        );
        let hit = traverse_blocks(
            Vec3::new(0.5, 0.5, 0.5),
            Vec3::new(0.5, 0.5, 9.5),
            |pos| (pos.z == 3).then_some(pos),
            || BetterBlockPos::ORIGIN,
        );
        assert_eq!(hit, BetterBlockPos::new(0, 0, 3));
    }
}
