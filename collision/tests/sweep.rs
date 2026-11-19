//! 碰撞层 golden tests。全部是定点数的整数断言，
//! 不存在"跑起来看着对"这种验收方式。

use collision::{sweep_capsule_aabb, sweep_point_aabb, Aabb, Bvh};
use sim_core::fixed::{Fix, Fix3};

fn v(x: i32, y: i32, z: i32) -> Fix3 {
    Fix3::new(Fix::from_int(x), Fix::from_int(y), Fix::from_int(z))
}

fn unit_box_at_origin() -> Aabb {
    Aabb::from_center_half(Fix3::ZERO, v(1, 1, 1))
}

#[test]
fn slab_hits_at_expected_parameter() {
    let b = unit_box_at_origin();
    let p0 = v(-10, 0, 0);
    let d = v(20, 0, 0);
    let t = sweep_point_aabb(p0, d, &b).expect("必须命中");

    // 手算：(-1 - (-10)) / 20 = 9/20 = 0.45
    // Q16.16 整数除：(9 << 16) / 20 = 29491（向零截断）
    assert_eq!(t.raw(), 29491);
}

#[test]
fn parallel_axis_outside_slab_misses() {
    let b = unit_box_at_origin();
    // y 方向速度为 0，且起点 y=10 在 [-1,1] 之外 → 永不相交
    let t = sweep_point_aabb(v(-10, 10, 0), v(20, 0, 0), &b);
    assert!(t.is_none());
}

#[test]
fn radius_makes_hit_earlier() {
    let b = unit_box_at_origin();
    let p0 = v(-10, 0, 0);
    let d = v(20, 0, 0);

    let t0 = sweep_capsule_aabb(p0, d, Fix::ZERO, &b).unwrap();
    let t3 = sweep_capsule_aabb(p0, d, Fix::from_int(3), &b).unwrap();

    assert!(t3.raw() < t0.raw(), "有半径必须更早接触");
    // (-4 - (-10)) / 20 = 0.3 → (6 << 16) / 20 = 19660
    assert_eq!(t3.raw(), 19660);
}

#[test]
fn sweep_behind_origin_misses() {
    let b = unit_box_at_origin();
    // 起点在盒子右侧，朝右走 → 永远碰不到
    assert!(sweep_point_aabb(v(10, 0, 0), v(20, 0, 0), &b).is_none());
}

#[test]
fn bvh_finds_nearest_of_many() {
    // 64 个盒子排成一行，间距 4
    let boxes: Vec<Aabb> = (0..64)
        .map(|i| Aabb::from_center_half(v(i * 4, 0, 0), v(1, 1, 1)))
        .collect();

    let bvh = Bvh::build(&boxes);
    assert!(bvh.node_count() > 1, "广相必须真的建了树");

    let hit = bvh
        .sweep_nearest(&boxes, v(-10, 0, 0), v(40, 0, 0), Fix::ZERO)
        .expect("这条线上必须命中");

    assert_eq!(hit.index, 0, "最近的一定是 x=0 那个");
}

#[test]
fn bvh_empty_is_safe() {
    let bvh = Bvh::build(&[]);
    assert_eq!(bvh.node_count(), 0);
    assert!(bvh.sweep_nearest(&[], v(0, 0, 0), v(1, 0, 0), Fix::ZERO).is_none());
}

#[test]
fn bvh_matches_bruteforce() {
    let boxes: Vec<Aabb> = (0..37)
        .map(|i| {
            Aabb::from_center_half(
                v((i * 7) % 23 - 11, (i * 5) % 17 - 8, (i * 3) % 13 - 6),
                v(1, 1, 1),
            )
        })
        .collect();

    let bvh = Bvh::build(&boxes);
    let p0 = v(-20, 0, 0);
    let d = v(40, 0, 0);

    let mut tree: Vec<u32> = bvh.sweep(&boxes, p0, d, Fix::ZERO).into_iter().map(|h| h.index).collect();
    tree.sort_unstable();

    let mut brute: Vec<u32> = Vec::new();
    for (i, b) in boxes.iter().enumerate() {
        if sweep_point_aabb(p0, d, b).is_some() {
            brute.push(i as u32);
        }
    }
    brute.sort_unstable();

    assert_eq!(tree, brute, "BVH 结果必须和暴力遍历完全一致");
}
