# 《九十秒》—— 模拟内核

确定性优先的开放世界内核。不是引擎，不是渲染器：**只有时间、时刻表、和一层可断言的碰撞。**

```
sim-core/    零依赖。定点数、时刻表、tick、128-bit 状态哈希
collision/   零依赖（仅依赖 sim-core）。广相 BVH + 胶囊 sweep
```

渲染（Three.js / WebGPU）和 impostor 烘焙器不在这个仓库里，因为它们**不影响确定性**，
也就不配进这个 workspace。

## 三条不变量

整套设计立在这三条上，任何一条破了，回放就分叉：

1. **没有浮点。** 全 Q16.16 定点。WASM 的 `+ - * /` 是 IEEE-754 定死的，但
   `sin/cos/pow` 走各运行时的 libm，跨平台结果不一致——所以本仓库不提供超越函数。
2. **世界只读。** `World` 里的节点和时刻表在运行期不可变。唯一在动的是 `Sim.tick`。
3. **没有随机。** 没有 `rand`，没有系统时间，没有 HashMap 迭代序。
   "某个 tick 发生什么"完全由时刻表决定。

## 时刻表长什么样

一个 NPC 的一天 = 一串首尾相接的 `Span`：

```rust
Span { t0: 0, t1: 100, from: 0, to: 1, state: Walk }
Span { t0: 100, t1: 200, from: 1, to: 2, state: Stay }
```

三条不变量由 `NpcTimeline::build` 在构建期强制校验（乱序 / 缺口 / 空区间一律返回 `Err`，
进不了运行时）。

运行时**不做寻路**，只有一次二分 + 一次定点插值：

```rust
let i = self.spans.partition_point(|s| s.t0 <= t);
let s = &self.spans[i - 1];              // O(log n) 定位
Fix3::lerp(nodes[s.from], nodes[s.to], Fix::ratio(t - s.t0, s.t1 - s.t0))
```

3000 个 NPC × 20Hz 就这样跑满，且结果逐位可复现。

## 回放怎么验

```rust
pub fn hash_chain(world: &World, ticks: u32) -> Vec<StateHash>
```

每个 tick 一个 128-bit 哈希。CI 每天干的事就是：

- 两个平台各跑 10k tick
- 两条链做 `==`
- 第一个不等的位置，就是 bug 的位置

快照是定长的：`8 + N × 15` 字节（tick + 每 NPC 的 id/xyz/state）。
3000 NPC ≈ 45KB/tick。断点续跑、云存档、"幽灵重放"共用同一个 `resume()`。

## 跑起来

```bash
cargo test              # 全部 golden test
cargo test -p sim-core  # 只跑内核
```

`rust-toolchain.toml` 里的版本是**故意 pin 死**的。换编译器版本必须先跑一遍
哈希链，对不上就不许升——这不是洁癖，这是回放能当验收标准的前提。

## 交给 Bot 的边界

| 能碰 | 不能碰 |
|---|---|
| `collision/` 全部 | `sim-core/src/timetable.rs` 的数据结构 |
| 测试用例与 fixture | 任何 tick 内的求解顺序 |
| BVH 的切分策略 | `Fix` 的位宽与舍入方向 |

判断标准：**能不能写出一个断言来判对错。** 写得出来的，归 Bot；写不出来的，归人。
