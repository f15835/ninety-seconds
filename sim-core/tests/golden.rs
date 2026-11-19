//! Golden tests。这一份就是给 Bot 的验收标准。
//!
//! 判据不依赖人的眼睛：跑两遍，哈希必须逐位相同。

use sim_core::fixed::{Fix, Fix3};
use sim_core::timetable::{NpcTimeline, ScheduleError, Span};
use sim_core::{Sim, World};

fn node(x: i32, z: i32) -> Fix3 {
    Fix3::new(Fix::from_int(x), Fix::ZERO, Fix::from_int(z))
}

fn span(t0: u32, t1: u32, from: u16, to: u16, state: u8) -> Span {
    Span {
        t0,
        t1,
        from,
        to,
        state,
    }
}

/// 5 个 anchor、3 个 NPC、200 tick 的一天。
fn fixture() -> World {
    let nodes = vec![
        node(0, 0),
        node(40, 0),
        node(40, 30),
        node(-10, 30),
        node(0, 60),
    ];

    let a = NpcTimeline::build(
        0,
        vec![
            span(0, 100, 0, 1, 0),
            span(100, 200, 1, 2, 1),
        ],
    )
    .unwrap();

    let b = NpcTimeline::build(
        1,
        vec![
            span(0, 150, 2, 3, 0),
            span(150, 200, 3, 4, 2),
        ],
    )
    .unwrap();

    let c = NpcTimeline::build(
        2,
        vec![
            span(0, 50, 4, 0, 3),
            span(50, 120, 0, 2, 0),
            span(120, 200, 2, 4, 0),
        ],
    )
    .unwrap();

    let w = World::new(nodes, vec![a, b, c]);
    sim_core::validate(&w);
    w
}

#[test]
fn same_input_same_hash() {
    let w = fixture();
    let a = sim_core::hash_chain(&w, 500);
    let b = sim_core::hash_chain(&w, 500);
    assert_eq!(a, b, "两条独立跑出来的哈希链必须逐位相同");
    assert_eq!(a.len(), 501);
}

#[test]
fn different_world_different_hash() {
    let w1 = fixture();
    let mut w2 = fixture();
    w2.nodes[3] = node(-11, 30); // 挪一个 anchor 一个单位
    let a = sim_core::hash_chain(&w1, 200);
    let b = sim_core::hash_chain(&w2, 200);
    assert_ne!(a, b, "改了输入，哈希链必须分叉");
}

#[test]
fn snapshot_roundtrip_is_lossless() {
    let w = fixture();
    let sim = Sim::initial(&w);
    let bytes = sim.encode();
    assert_eq!(bytes.len(), 8 + 3 * sim_core::tick::POSE_BYTES);

    let back = Sim::decode(&bytes).expect("快照必须能解回来");
    assert_eq!(sim.tick, back.tick);
    assert_eq!(sim.hash(), back.hash(), "编解码必须无损");
}

#[test]
fn resume_from_snapshot_equals_continuous_run() {
    let w = fixture();
    let full = sim_core::hash_chain(&w, 180);

    let snap = Sim::initial(&w).encode();
    let resumed = sim_core::resume(&w, &snap, 180).expect("快照可续跑");
    assert_eq!(full.last().unwrap(), &resumed, "断点续跑的终点必须一致");
}

#[test]
fn replay_from_mid_snapshot_matches_suffix() {
    let w = fixture();
    let mut sim = Sim::initial(&w);
    for _ in 0..137 {
        sim.step(&w);
    }
    let mid = sim.encode();
    let from_mid = sim_core::resume(&w, &mid, 53).unwrap();

    let chain = sim_core::hash_chain(&w, 190);
    assert_eq!(
        &from_mid,
        chain.last().unwrap(),
        "从第 137 tick 续跑 53 步应等于连续跑 190 步"
    );
}

#[test]
fn timetable_rejects_gap_and_unsorted() {
    // 时间有缺口
    let bad = NpcTimeline::build(0, vec![span(0, 100, 0, 1, 0), span(101, 200, 1, 2, 0)]);
    assert_eq!(bad.unwrap_err(), ScheduleError::Gap { npc: 0, at: 1 });

    // 乱序
    let bad = NpcTimeline::build(0, vec![span(100, 200, 1, 2, 0), span(0, 100, 0, 1, 0)]);
    assert_eq!(
        bad.unwrap_err(),
        ScheduleError::Unsorted { npc: 0, at: 1 }
    );

    // 空区间
    let bad = NpcTimeline::build(0, vec![span(0, 0, 0, 1, 0)]);
    assert_eq!(bad.unwrap_err(), ScheduleError::EmptySpan { npc: 0, at: 0 });
}

#[test]
fn span_at_is_half_open() {
    let tl = NpcTimeline::build(0, vec![span(0, 100, 0, 1, 0), span(100, 200, 1, 2, 1)]).unwrap();

    assert_eq!(tl.span_at(0).unwrap().from, 0);
    assert_eq!(tl.span_at(99).unwrap().from, 0);
    // t1 是开区间：100 属于下一段
    assert_eq!(tl.span_at(100).unwrap().from, 1);
    assert_eq!(tl.span_at(199).unwrap().from, 1);
    // 一天结束后没有归属
    assert!(tl.span_at(200).is_none());
}

#[test]
fn pose_interpolates_exactly_midway() {
    let nodes = vec![node(0, 0), node(40, 0)];
    let tl = NpcTimeline::build(0, vec![span(0, 100, 0, 1, 0)]).unwrap();
    let p = tl.pose_at(50, &nodes).unwrap();
    // 40 * 0.5 = 20，定点下必须精确命中
    assert_eq!(p.pos.x, Fix::from_int(20));
}
