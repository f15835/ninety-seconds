//! 《九十秒》确定性模拟内核。
//!
//! 对外只暴露四样东西：定点数、时刻表、Sim 快照、哈希。
//! 别的都不导出 —— 边界越窄，Bot 能碰坏的面积越小。

pub mod fixed;
pub mod hash;
pub mod tick;
pub mod timetable;

pub use fixed::{Fix, Fix3};
pub use hash::StateHash;
pub use tick::{hash_chain, resume, Sim, TICK_HZ, WINDOW_TICKS};
pub use timetable::{NpcId, NpcTimeline, NodeId, Pose, Span, Tick, World};

/// 构建期自检：跨表不变量。
/// 单个 NPC 内部的不变量已由 `NpcTimeline::build` 保证，这里只管跨 NPC 的。
pub fn validate(world: &World) {
    let mut prev: Option<NpcId> = None;
    for tl in &world.npcs {
        assert!(
            prev.map_or(true, |p| tl.npc > p),
            "NPC id 必须严格递增且唯一（发现 {}）",
            tl.npc
        );
        prev = Some(tl.npc);

        // 每个 span 引用的 anchor 必须真实存在。否则运行时会静默落到
        // 原点 —— 画面上表现为"有人站在世界原点"，极难定位。
        for s in tl.spans() {
            assert!(
                (s.from as usize) < world.nodes.len(),
                "npc {} 第 t={} 段引用了不存在的节点 {}",
                tl.npc,
                s.t0,
                s.from
            );
            assert!(
                (s.to as usize) < world.nodes.len(),
                "npc {} 第 t={} 段引用了不存在的节点 {}",
                tl.npc,
                s.t0,
                s.to
            );
        }
    }
}
