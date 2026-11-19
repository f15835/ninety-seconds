//! tick 循环、快照编解码、回放校验。
//!
//! 确定性回放的全部秘密只有三条：
//!   1. 输入 = (初始快照, tick 号)，没有别的。世界是只读的。
//!   2. 每 tick 结束算一次 128-bit 状态哈希。
//!   3. 回放 = 重跑，然后逐 tick 比对哈希链。
//!
//! 没有随机数、没有浮点、没有系统时间、没有 HashMap 迭代顺序 ——
//! 任何一条破了，哈希链就会分叉，CI 当晚就红。

use crate::fixed::Fix3;
use crate::hash::StateHash;
use crate::timetable::{Pose, Tick, World};

pub const TICK_HZ: u32 = 20;
/// 90 秒 = 1800 tick。主角的时间预算就是这个世界的一格。
pub const WINDOW_TICKS: u32 = 90 * TICK_HZ;

/// 单个 Pose 的定长编码：2 + 4 + 4 + 4 + 1 = 15 字节。
/// 3000 NPC → 45KB/tick 快照。8MB 的上限留给多帧环形缓冲。
pub const POSE_BYTES: usize = 15;

#[derive(Clone, Debug)]
pub struct Sim {
    pub tick: Tick,
    pub poses: Vec<Pose>,
}

impl Sim {
    pub fn initial(world: &World) -> Sim {
        let mut poses = vec![
            Pose {
                npc: 0,
                pos: Fix3::ZERO,
                state: 0,
            };
            world.npcs.len()
        ];
        world.sample(0, &mut poses);
        Sim { tick: 0, poses }
    }

    /// 前进一步。这是整个游戏里唯一的「时间流逝」。
    #[inline]
    pub fn step(&mut self, world: &World) {
        self.tick += 1;
        world.sample(self.tick, &mut self.poses);
    }

    pub fn hash(&self) -> StateHash {
        let mut h = StateHash::SEED;
        h.mix_u32(self.tick);
        for p in &self.poses {
            h.mix_u16(p.npc);
            h.mix_fix3(p.pos);
            h.mix_byte(p.state);
        }
        h
    }

    /// 快照编码。定长、无版本头、无 serde —— 热路径上不要派生宏。
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.poses.len() * POSE_BYTES);
        out.extend_from_slice(&self.tick.to_le_bytes());
        out.extend_from_slice(&(self.poses.len() as u32).to_le_bytes());
        for p in &self.poses {
            out.extend_from_slice(&p.npc.to_le_bytes());
            out.extend_from_slice(&p.pos.x.raw().to_le_bytes());
            out.extend_from_slice(&p.pos.y.raw().to_le_bytes());
            out.extend_from_slice(&p.pos.z.raw().to_le_bytes());
            out.push(p.state);
        }
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Sim> {
        if bytes.len() < 8 {
            return None;
        }
        let tick = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let n = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
        if bytes.len() != 8 + n * POSE_BYTES {
            return None;
        }
        let mut poses = Vec::with_capacity(n);
        for i in 0..n {
            let base = 8 + i * POSE_BYTES;
            let npc = u16::from_le_bytes(bytes[base..base + 2].try_into().ok()?);
            let x = i32::from_le_bytes(bytes[base + 2..base + 6].try_into().ok()?);
            let y = i32::from_le_bytes(bytes[base + 6..base + 10].try_into().ok()?);
            let z = i32::from_le_bytes(bytes[base + 10..base + 14].try_into().ok()?);
            let state = bytes[base + 14];
            poses.push(Pose {
                npc,
                pos: Fix3 {
                    x: crate::fixed::Fix::from_raw(x),
                    y: crate::fixed::Fix::from_raw(y),
                    z: crate::fixed::Fix::from_raw(z),
                },
                state,
            });
        }
        Some(Sim { tick, poses })
    }
}

/// 跑 n 个 tick，返回每个 tick 的哈希链。
/// CI 就是拿两条链做 `==`，第一个不等的位置就是 bug 的位置。
pub fn hash_chain(world: &World, ticks: u32) -> Vec<StateHash> {
    let mut sim = Sim::initial(world);
    let mut chain = Vec::with_capacity(ticks as usize + 1);
    chain.push(sim.hash());
    for _ in 0..ticks {
        sim.step(world);
        chain.push(sim.hash());
    }
    chain
}

/// 从任意快照继续跑 —— 存档、断线重连、"幽灵重放"都走这个入口。
pub fn resume(world: &World, snapshot: &[u8], ticks: u32) -> Option<StateHash> {
    let mut sim = Sim::decode(snapshot)?;
    for _ in 0..ticks {
        sim.step(world);
    }
    Some(sim.hash())
}
