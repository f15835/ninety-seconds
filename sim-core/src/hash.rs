//! 128-bit 状态哈希（双通道 FNV-1a）。
//!
//! 用途：每个 tick 结束把整个世界的状态压成 16 字节。
//! 回放校验只需要比对这 16 字节，不需要比对整个快照 —— 这是 nightly
//! 跨平台一致性检查能在几分钟内跑完 10k tick 的原因。
//!
//! 为什么不用 DefaultHasher：它的算法不保证跨 Rust 版本稳定。

#[derive(Copy, Clone, PartialEq, Eq, Debug, Default)]
pub struct StateHash {
    pub lo: u64,
    pub hi: u64,
}

const OFF_LO: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME_LO: u64 = 0x0000_0100_0000_01b3;
const OFF_HI: u64 = 0x9e37_79b9_7f4a_7c15;
const PRIME_HI: u64 = 0xff51_afd7_ed55_8ccd;

impl StateHash {
    pub const SEED: StateHash = StateHash {
        lo: OFF_LO,
        hi: OFF_HI,
    };

    #[inline]
    pub fn mix_byte(&mut self, b: u8) {
        self.lo = (self.lo ^ b as u64).wrapping_mul(PRIME_LO);
        self.hi = (self.hi ^ b as u64).wrapping_mul(PRIME_HI);
    }

    #[inline]
    pub fn mix_u16(&mut self, v: u16) {
        self.mix_byte((v & 0xff) as u8);
        self.mix_byte((v >> 8) as u8);
    }

    #[inline]
    pub fn mix_i32(&mut self, v: i32) {
        let b = v.to_le_bytes();
        let mut i = 0usize;
        while i < 4 {
            self.mix_byte(b[i]);
            i += 1;
        }
    }

    #[inline]
    pub fn mix_u32(&mut self, v: u32) {
        self.mix_i32(v as i32);
    }

    pub fn mix_fix3(&mut self, p: crate::fixed::Fix3) {
        self.mix_i32(p.x.raw());
        self.mix_i32(p.y.raw());
        self.mix_i32(p.z.raw());
    }
}

impl core::fmt::Display for StateHash {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:016x}{:016x}", self.hi, self.lo)
    }
}
