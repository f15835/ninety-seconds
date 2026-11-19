//! 碰撞层。这是交给 DevinBot 的第一块硬活。
//!
//! 为什么敢整块外包：这里的每一个函数都能被写成
//! `assert_eq!(result, 手算的定点数)`。没有"手感"、没有"看起来对不对"，
//! 只有对和错。Bot 在这里不需要人打分，它自己跑测试就知道。
//!
//! 结构：
//!   广相  BVH（中位数切分，叶子 4 个）
//!   窄相  Minkowski 展开 + slab 扫掠
//!   胶囊  不是特殊形状 —— 胶囊 = 把 AABB 按半径膨胀后的点扫掠。
//!         这一刀让窄相代码量降到原来的三分之一。

use sim_core::fixed::{Fix, Fix3};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Aabb {
    pub min: Fix3,
    pub max: Fix3,
}

impl Aabb {
    pub fn from_center_half(c: Fix3, h: Fix3) -> Aabb {
        Aabb {
            min: c.sub(h),
            max: c.add(h),
        }
    }

    pub fn union(self, o: Aabb) -> Aabb {
        Aabb {
            min: Fix3 {
                x: if o.min.x < self.min.x { o.min.x } else { self.min.x },
                y: if o.min.y < self.min.y { o.min.y } else { self.min.y },
                z: if o.min.z < self.min.z { o.min.z } else { self.min.z },
            },
            max: Fix3 {
                x: if o.max.x > self.max.x { o.max.x } else { self.max.x },
                y: if o.max.y > self.max.y { o.max.y } else { self.max.y },
                z: if o.max.z > self.max.z { o.max.z } else { self.max.z },
            },
        }
    }

    /// Minkowski 膨胀。胶囊半径就靠它进到 slab 里。
    pub fn expanded(self, r: Fix) -> Aabb {
        let e = Fix3 {
            x: r,
            y: r,
            z: r,
        };
        Aabb {
            min: self.min.sub(e),
            max: self.max.add(e),
        }
    }

    pub fn centroid(self) -> Fix3 {
        Fix3 {
            x: mid(self.min.x, self.max.x),
            y: mid(self.min.y, self.max.y),
            z: mid(self.min.z, self.max.z),
        }
    }

    pub fn longest_axis(self) -> u8 {
        let dx = self.max.x.raw() - self.min.x.raw();
        let dy = self.max.y.raw() - self.min.y.raw();
        let dz = self.max.z.raw() - self.min.z.raw();
        if dx >= dy && dx >= dz {
            0
        } else if dy >= dz {
            1
        } else {
            2
        }
    }
}

#[inline]
fn mid(a: Fix, b: Fix) -> Fix {
    Fix::from_raw((((a.raw() as i64) + (b.raw() as i64)) / 2) as i32)
}

// ───────────────────────────── 窄相 ─────────────────────────────

/// 点沿 d 扫掠 vs AABB（slab 法）。返回首次接触参数 t ∈ [0,1]。
///
/// 定点版，无浮点，无除法零险（dv 为 0 走平行分支）。
pub fn sweep_point_aabb(p0: Fix3, d: Fix3, b: &Aabb) -> Option<Fix> {
    let mut t_enter = Fix::ZERO;
    let mut t_exit = Fix::ONE;

    let axes = [
        (p0.x, d.x, b.min.x, b.max.x),
        (p0.y, d.y, b.min.y, b.max.y),
        (p0.z, d.z, b.min.z, b.max.z),
    ];

    for (p, dv, lo, hi) in axes {
        if dv.raw() == 0 {
            // 平行于该轴：起点必须已在板内，否则永不相交
            if p.raw() < lo.raw() || p.raw() > hi.raw() {
                return None;
            }
        } else {
            let mut t1 = (lo - p).div(dv);
            let mut t2 = (hi - p).div(dv);
            if t1 > t2 {
                core::mem::swap(&mut t1, &mut t2);
            }
            if t1 > t_enter {
                t_enter = t1;
            }
            if t2 < t_exit {
                t_exit = t2;
            }
            if t_enter > t_exit {
                return None;
            }
        }
    }

    if t_exit < Fix::ZERO || t_enter > Fix::ONE {
        None
    } else {
        Some(t_enter)
    }
}

/// 胶囊扫掠。胶囊 = 点 + 半径，AABB = AABB，所以 = 点 vs 膨胀后的 AABB。
/// 这个恒等式是整个碰撞层能砍到 7k 行的原因。
pub fn sweep_capsule_aabb(p0: Fix3, d: Fix3, radius: Fix, b: &Aabb) -> Option<Fix> {
    sweep_point_aabb(p0, d, &b.expanded(radius))
}

// ───────────────────────────── 广相 ─────────────────────────────

#[derive(Copy, Clone, Debug)]
struct Node {
    aabb: Aabb,
    left: u32,
    right: u32,
    start: u32,
    count: u32,
}

#[derive(Copy, Clone, Debug)]
pub struct Hit {
    pub index: u32,
    pub t: Fix,
}

pub struct Bvh {
    nodes: Vec<Node>,
    order: Vec<u32>,
}

const LEAF_SIZE: usize = 4;

impl Bvh {
    pub fn build(boxes: &[Aabb]) -> Bvh {
        let mut order: Vec<u32> = (0..boxes.len() as u32).collect();
        let mut nodes: Vec<Node> = Vec::with_capacity(boxes.len().max(1) * 2);
        if boxes.is_empty() {
            return Bvh { nodes, order };
        }
        let n = boxes.len();
        build_node(boxes, &mut order, 0, n, &mut nodes);
        Bvh { nodes, order }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 查询：把胶囊扫掠过 BVH，收集所有命中（带 t），按 t 升序。
    pub fn sweep(&self, boxes: &[Aabb], p0: Fix3, d: Fix3, radius: Fix) -> Vec<Hit> {
        let mut out = Vec::new();
        if self.nodes.is_empty() {
            return out;
        }
        descend(
            self,
            0,
            boxes,
            p0,
            d,
            radius,
            &mut out,
        );
        out.sort_by_key(|h| h.t.raw());
        out
    }

    /// 只取最近的一个 —— 90 秒里绝大多数查询只关心这个。
    pub fn sweep_nearest(&self, boxes: &[Aabb], p0: Fix3, d: Fix3, radius: Fix) -> Option<Hit> {
        let mut out = Vec::new();
        if self.nodes.is_empty() {
            return None;
        }
        descend(self, 0, boxes, p0, d, radius, &mut out);
        out.into_iter().min_by_key(|h| h.t.raw())
    }
}

fn build_node(
    boxes: &[Aabb],
    order: &mut Vec<u32>,
    lo: usize,
    hi: usize,
    nodes: &mut Vec<Node>,
) -> u32 {
    let idx = nodes.len() as u32;

    let mut aabb = boxes[order[lo] as usize];
    for k in (lo + 1)..hi {
        aabb = aabb.union(boxes[order[k] as usize]);
    }

    nodes.push(Node {
        aabb,
        left: u32::MAX,
        right: u32::MAX,
        start: lo as u32,
        count: (hi - lo) as u32,
    });

    if hi - lo <= LEAF_SIZE {
        return idx;
    }

    let axis = aabb.longest_axis();
    order[lo..hi].sort_by_key(|&i| boxes[i as usize].centroid().axis(axis).raw());

    let mid = lo + (hi - lo) / 2;
    let l = build_node(boxes, order, lo, mid, nodes);
    let r = build_node(boxes, order, mid, hi, nodes);

    // 内部节点不保留 start/count（叶子才用）
    nodes[idx as usize].left = l;
    nodes[idx as usize].right = r;
    nodes[idx as usize].start = 0;
    nodes[idx as usize].count = 0;
    idx
}

fn descend(
    bvh: &Bvh,
    node: u32,
    boxes: &[Aabb],
    p0: Fix3,
    d: Fix3,
    radius: Fix,
    out: &mut Vec<Hit>,
) {
    let n = bvh.nodes[node as usize];

    // 广相也走同一个函数：整棵子树一次扫掠判定，直接剪掉。
    if sweep_capsule_aabb(p0, d, radius, &n.aabb).is_none() {
        return;
    }

    if n.left == u32::MAX {
        let s = n.start as usize;
        let e = s + n.count as usize;
        for &bi in bvh.order[s..e].iter() {
            if let Some(t) = sweep_capsule_aabb(p0, d, radius, &boxes[bi as usize]) {
                out.push(Hit { index: bi, t });
            }
        }
    } else {
        descend(bvh, n.left, boxes, p0, d, radius, out);
        descend(bvh, n.right, boxes, p0, d, radius, out);
    }
}
