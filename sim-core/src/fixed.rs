//! Q16.16 定点数。
//!
//! 模拟层内一律禁用 f32/f64。原因不是洁癖：
//! 1. WASM 的 `+ - * / sqrt` 是 IEEE-754 定死的，但 `sin/cos/pow/exp`
//!    走各运行时的 libm 实现，不同版本不同平台结果不一致 —— 一用，
//!    回放就崩。所以本文件不提供任何超越函数。
//! 2. 定点数的乘法/除法是整数运算，跨平台逐位相同，可以直接写 golden test。

use core::ops::{Add, Div, Mul, Neg, Sub};

pub const FRAC_BITS: u32 = 16;
pub const ONE_RAW: i32 = 1 << FRAC_BITS;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fix(i32);

impl Fix {
    pub const ZERO: Fix = Fix(0);
    pub const ONE: Fix = Fix(ONE_RAW);

    pub const fn from_raw(raw: i32) -> Fix {
        Fix(raw)
    }
    pub const fn raw(self) -> i32 {
        self.0
    }
    pub const fn from_int(v: i32) -> Fix {
        Fix(v.wrapping_mul(ONE_RAW))
    }
    pub const fn to_int(self) -> i32 {
        self.0 >> FRAC_BITS
    }

    /// 整数比 num/den 转定点。时刻表插值只用这一个入口，
    /// 不经过浮点，也不会遇到除零（den 为 0 由调用方挡）。
    pub fn ratio(num: u32, den: u32) -> Fix {
        debug_assert!(den != 0);
        Fix((((num as i64) << FRAC_BITS) / (den as i64)) as i32)
    }

    pub fn mul(self, rhs: Fix) -> Fix {
        Fix((((self.0 as i64) * (rhs.0 as i64)) >> FRAC_BITS) as i32)
    }

    pub fn div(self, rhs: Fix) -> Fix {
        debug_assert!(rhs.0 != 0);
        Fix((((self.0 as i64) << FRAC_BITS) / (rhs.0 as i64)) as i32)
    }

    /// 确定性线性插值：t 固定落在 [0,1]，结果只由整数运算决定。
    pub fn lerp(a: Fix, b: Fix, t: Fix) -> Fix {
        a + (b - a).mul(t)
    }

    /// 整数牛顿迭代开方。碰撞层算胶囊半径距离要用，
    /// 但绝不能用 sqrtf —— 见文件头。
    pub fn isqrt(self) -> Fix {
        debug_assert!(self.0 >= 0, "isqrt 只接受非负数");
        let v = (self.0 as u64) << FRAC_BITS; // 值 * 2^16
        if v == 0 {
            return Fix::ZERO;
        }
        let mut x = v;
        let mut y = (x + 1) / 2;
        while y < x {
            x = y;
            y = (x + v / x) / 2;
        }
        Fix(x as i32)
    }
}

impl Add for Fix {
    type Output = Fix;
    #[inline]
    fn add(self, o: Fix) -> Fix {
        Fix(self.0 + o.0)
    }
}
impl Sub for Fix {
    type Output = Fix;
    #[inline]
    fn sub(self, o: Fix) -> Fix {
        Fix(self.0 - o.0)
    }
}
impl Neg for Fix {
    type Output = Fix;
    #[inline]
    fn neg(self) -> Fix {
        Fix(-self.0)
    }
}
impl Mul for Fix {
    type Output = Fix;
    #[inline]
    fn mul(self, o: Fix) -> Fix {
        Fix::mul(self, o)
    }
}
impl Div for Fix {
    type Output = Fix;
    #[inline]
    fn div(self, o: Fix) -> Fix {
        Fix::div(self, o)
    }
}

/// 世界坐标。时刻表里所有 anchor 节点、所有 NPC 位置都是它。
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Fix3 {
    pub x: Fix,
    pub y: Fix,
    pub z: Fix,
}

impl Fix3 {
    pub const ZERO: Fix3 = Fix3 {
        x: Fix::ZERO,
        y: Fix::ZERO,
        z: Fix::ZERO,
    };

    pub const fn new(x: Fix, y: Fix, z: Fix) -> Fix3 {
        Fix3 { x, y, z }
    }

    pub fn lerp(a: Fix3, b: Fix3, t: Fix) -> Fix3 {
        Fix3 {
            x: Fix::lerp(a.x, b.x, t),
            y: Fix::lerp(a.y, b.y, t),
            z: Fix::lerp(a.z, b.z, t),
        }
    }

    pub fn add(self, o: Fix3) -> Fix3 {
        Fix3 {
            x: self.x + o.x,
            y: self.y + o.y,
            z: self.z + o.z,
        }
    }

    pub fn sub(self, o: Fix3) -> Fix3 {
        Fix3 {
            x: self.x - o.x,
            y: self.y - o.y,
            z: self.z - o.z,
        }
    }

    pub fn scale(self, s: Fix) -> Fix3 {
        Fix3 {
            x: self.x.mul(s),
            y: self.y.mul(s),
            z: self.z.mul(s),
        }
    }

    pub fn axis(self, i: u8) -> Fix {
        match i {
            0 => self.x,
            1 => self.y,
            _ => self.z,
        }
    }
}
