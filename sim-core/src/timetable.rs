//! 时刻表：整座城市的唯一真相。
//!
//! 数据结构就一句话：
//!   NPC 的一天 = 一串按 t0 升序、首尾相接的 `Span`。
//!   每个 Span 只记「从哪个 anchor 节点走到哪个 anchor 节点、花多久、什么状态」。
//!
//! 运行时**不做寻路**。位置 = 二分查到所在 Span，再对两个 anchor 做定点插值。
//! 这就是为什么 3000 个 NPC 可以在 20Hz 下全量更新，也是为什么整个
//! 世界状态能被压成 (tick, poses) 这么薄的一层。

use crate::fixed::{Fix, Fix3, FRAC_BITS};
use crate::hash::StateHash;

pub type Tick = u32;
pub type NpcId = u16;
pub type NodeId = u16;

/// 一个 NPC 在 [t0, t1) 区间内的行为。
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub t0: Tick,
    pub t1: Tick,
    pub from: NodeId,
    pub to: NodeId,
    /// 0 = 步行, 1 = 停留, 2 = 乘车, 3 = 工作, 4 = 睡眠…
    /// 附身可用性、玩家可见性都由它派发，不是散在各处的 if。
    pub state: u8,
}

/// 单个 NPC 的完整一天。spans 不变量（由 `build` 强制校验）：
///   1. t0 严格递增
///   2. spans[i].t1 == spans[i+1].t0   —— 首尾相接，没有缝隙
///   3. 区间非空：t1 > t0
///
/// 这三条一旦成立，"某个 tick 在干什么" 就是一次二分，永远命中恰好一个 Span。
#[derive(Clone, Debug)]
pub struct NpcTimeline {
    pub npc: NpcId,
    spans: Vec<Span>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScheduleError {
    Unsorted { npc: NpcId, at: usize },
    Gap { npc: NpcId, at: usize },
    EmptySpan { npc: NpcId, at: usize },
}

impl core::fmt::Display for ScheduleError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ScheduleError::Unsorted { npc, at } => write!(f, "npc {npc} 第 {at} 段乱序"),
            ScheduleError::Gap { npc, at } => write!(f, "npc {npc} 第 {at} 段时间有缺口"),
            ScheduleError::EmptySpan { npc, at } => write!(f, "npc {npc} 第 {at} 段长度为 0"),
        }
    }
}

impl NpcTimeline {
    /// 唯一构造入口。校验不过直接进不了运行时 —— 时刻表是数据，
    /// 数据的错误必须在构建期炸，不能留到第 5 周。
    pub fn build(npc: NpcId, spans: Vec<Span>) -> Result<NpcTimeline, ScheduleError> {
        for i in 0..spans.len() {
            if spans[i].t1 <= spans[i].t0 {
                return Err(ScheduleError::EmptySpan { npc, at: i });
            }
            if i > 0 {
                if spans[i].t0 < spans[i - 1].t0 {
                    return Err(ScheduleError::Unsorted { npc, at: i });
                }
                if spans[i].t0 != spans[i - 1].t1 {
                    return Err(ScheduleError::Gap { npc, at: i });
                }
            }
        }
        Ok(NpcTimeline { npc, spans })
    }

    #[inline]
    pub fn span_count(&self) -> usize {
        self.spans.len()
    }

    /// 只读遍历，给构建期校验和导出工具用。
    #[inline]
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// O(log n) 定位。`partition_point` 找第一个 t0 > t 的位置，
    /// 前一个就是候选；再验 t < t1 防越界。
    pub fn span_at(&self, t: Tick) -> Option<&Span> {
        let i = self.spans.partition_point(|s| s.t0 <= t);
        if i == 0 {
            return None;
        }
        let s = &self.spans[i - 1];
        if t < s.t1 {
            Some(s)
        } else {
            None
        }
    }

    /// 位置与状态：一次二分 + 一次定点插值，没有浮点，没有寻路。
    pub fn pose_at(&self, t: Tick, nodes: &[Fix3]) -> Option<Pose> {
        let s = self.span_at(t)?;
        let from = *nodes.get(s.from as usize)?;
        let to = *nodes.get(s.to as usize)?;
        let den = s.t1 - s.t0;
        let alpha = Fix::ratio(t - s.t0, den);
        Some(Pose {
            npc: self.npc,
            pos: Fix3::lerp(from, to, alpha),
            state: s.state,
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Pose {
    pub npc: NpcId,
    pub pos: Fix3,
    pub state: u8,
}

/// 城市。两个数组都是只读的：运行时不存在「世界被修改」这件事，
/// 只有 tick 往前走。这是回放能成立的根本原因。
pub struct World {
    pub nodes: Vec<Fix3>,
    pub npcs: Vec<NpcTimeline>,
}

impl World {
    pub fn new(nodes: Vec<Fix3>, npcs: Vec<NpcTimeline>) -> World {
        World { nodes, npcs }
    }

    /// 全量采样一个 tick。3000 个 NPC = 3000 次二分，20Hz 下毫无压力。
    pub fn sample(&self, t: Tick, out: &mut [Pose]) {
        debug_assert!(out.len() >= self.npcs.len());
        for (i, tl) in self.npcs.iter().enumerate() {
            out[i] = tl.pose_at(t, &self.nodes).unwrap_or(Pose {
                npc: tl.npc,
                pos: Fix3::ZERO,
                state: 0,
            });
        }
    }

    pub fn hash_at(&self, t: Tick, poses: &[Pose]) -> StateHash {
        let mut h = StateHash::SEED;
        h.mix_u32(t);
        for p in poses.iter().take(self.npcs.len()) {
            h.mix_u16(p.npc);
            h.mix_fix3(p.pos);
            h.mix_byte(p.state);
        }
        h
    }

    /// 一天有多少 tick（20Hz）。
    pub fn day_ticks(&self) -> Tick {
        let mut end = 0;
        for tl in &self.npcs {
            if let Some(last) = tl.spans.last() {
                end = end.max(last.t1);
            }
        }
        end
    }
}

/// 给时刻表导出/诊断用：把 Fix 打成 "整数.小数" 便于人眼核对。
pub fn fmt_fix(v: Fix) -> String {
    let raw = v.raw();
    let int = raw >> FRAC_BITS;
    let frac = ((raw as i64 - ((int as i64) << FRAC_BITS)) * 1000) >> FRAC_BITS;
    format!("{int}.{frac:03}")
}
