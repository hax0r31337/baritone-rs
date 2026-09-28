// Ported from baritone src/main/java/baritone/utils/pathing/PathingBlockType.java @ 25111daedf1d59e6a8dfb5a3e61885cdb8d953df

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PathingBlockType {
    Air,
    Water,
    Avoid,
    Solid,
}

impl PathingBlockType {
    pub const VALUES: [PathingBlockType; 4] = [
        PathingBlockType::Air,
        PathingBlockType::Water,
        PathingBlockType::Avoid,
        PathingBlockType::Solid,
    ];

    fn bits_int(self) -> i32 {
        match self {
            PathingBlockType::Air => 0b00,
            PathingBlockType::Water => 0b01,
            PathingBlockType::Avoid => 0b10,
            PathingBlockType::Solid => 0b11,
        }
    }

    pub fn get_bits(self) -> [bool; 2] {
        let bits = self.bits_int();
        [(bits & 0b10) != 0, (bits & 0b01) != 0]
    }

    pub fn from_bits(b1: bool, b2: bool) -> PathingBlockType {
        if b1 {
            if b2 {
                PathingBlockType::Solid
            } else {
                PathingBlockType::Avoid
            }
        } else if b2 {
            PathingBlockType::Water
        } else {
            PathingBlockType::Air
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from baritone src/test/java/baritone/utils/pathing/PathingBlockTypeTest.java
    #[test]
    fn test_bits() {
        for ty in PathingBlockType::VALUES {
            let bits = ty.get_bits();
            assert_eq!(ty, PathingBlockType::from_bits(bits[0], bits[1]));
        }
    }
}
