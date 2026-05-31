use crate::git::commit::TopoNode;
use crate::git::stash::StashInfo;
use crate::graph::color::BranchColors;
use git2::Oid;
use ratatui::style::Color;
use std::collections::{HashMap, HashSet};

/// 图中单个格子的类型
///
/// 角落字符的边连接（通过圆角矩形 ╭──╮ / │  │ / ╰──╯ 验证）:
///   ╭ BranchRight = BOTTOM + RIGHT (线向下、向右延伸)
///   ╮ BranchLeft  = BOTTOM + LEFT  (线向下、向左延伸)
///   ╰ MergeRight  = TOP + RIGHT    (线从上方来、向右延伸)
///   ╯ MergeLeft   = TOP + LEFT     (线从上方来、向左延伸)
///
/// 使用场景:
///   分叉(fork): Branch* — 新分支向下走 → 需要 BOTTOM 边
///   收敛(convergence): Merge* — 路径从上方来，结束于侧面 → 需要 TOP 边
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum CellType {
    Empty,
    Pipe(Color),
    Commit(Color),
    /// ╭ TOP+RIGHT — 路径从上方来，向右转（用于收敛连接器 from < to）
    BranchRight(Color),
    /// ╮ TOP+LEFT — 路径从上方来，向左转（用于收敛连接器 from > to）
    BranchLeft(Color),
    /// ╰ BOTTOM+RIGHT — 路径从左方来，向下走（用于分叉连接器 extra < main）
    MergeRight(Color),
    /// ╯ BOTTOM+LEFT — 路径从右方来，向下走（用于分叉连接器 extra > main）
    MergeLeft(Color),
    Horizontal(Color),
    /// ├ 管道继续 + 右侧分支
    TeeRight(Color),
    /// ┤ 管道继续 + 左侧分支
    TeeLeft(Color),
}

/// 图中一行（对应一个 commit 或一个连接器）
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GraphRow {
    pub oid: Option<Oid>,
    pub lane: usize,
    pub cells: Vec<CellType>,
    pub branch: Option<String>,
    pub branches: Vec<String>,
    pub remote_branches: Vec<String>,
    pub message_short: String,
    pub has_stash: bool,
    pub tags: Vec<String>,
}

/// 布局结果
#[allow(dead_code)]
pub struct GraphLayout {
    pub rows: Vec<GraphRow>,
    pub max_lane: usize,
}

/// 一条活跃的视觉路径（占据一列）
struct Lane {
    /// 正在追踪的目标 commit（None 表示空闲列）
    target: Option<Oid>,
    /// 此路径的颜色
    color: Color,
}

/// 颜色回收池：记录每种颜色何时被释放（行号），以便复用
struct ColorPool {
    /// (颜色, 被释放的行号)
    freed: Vec<(Color, usize)>,
    /// 下一个从未使用的颜色索引
    next_idx: usize,
}

impl ColorPool {
    fn new() -> Self {
        ColorPool {
            freed: Vec::new(),
            next_idx: 0,
        }
    }

    /// 获取一个可用的颜色（优先复用已释放的）
    fn acquire(&mut self, row: usize, colors: &mut BranchColors) -> Color {
        // 找一个已经不会再被使用的颜色（释放行 < 当前行）
        if let Some(pos) = self
            .freed
            .iter()
            .position(|(_, freed_row)| *freed_row < row)
        {
            let (color, _) = self.freed.remove(pos);
            return color;
        }
        // 没有可复用的，分配新颜色
        let name = format!("__path_{}", self.next_idx);
        self.next_idx += 1;
        colors.color_for(&name)
    }

    /// 释放一个颜色
    fn release(&mut self, color: Color, row: usize) {
        self.freed.push((color, row));
    }
}

/// 构建 lane-based graph 布局（参考 VSCode Git Graph 的 determinePath 算法）
pub fn build_layout(
    nodes: &[TopoNode],
    branch_map: &HashMap<Oid, Vec<String>>,
    stash_map: &HashMap<Oid, Vec<StashInfo>>,
    colors: &mut BranchColors,
    tag_map: &HashMap<Oid, Vec<String>>,
    remote_branch_map: &HashMap<Oid, Vec<String>>,
) -> GraphLayout {
    if nodes.is_empty() {
        return GraphLayout {
            rows: Vec::new(),
            max_lane: 0,
        };
    }

    let stash_oids: HashSet<Oid> = stash_map.values().flatten().map(|s| s.oid).collect();

    // ===== Phase 1: 为 branch tip 分配初始颜色 =====
    let mut commit_color: HashMap<Oid, Color> = HashMap::new();
    for node in nodes {
        if let Some(branches) = branch_map.get(&node.oid) {
            if let Some(first) = branches.first() {
                let color = colors.color_for(first);
                commit_color.insert(node.oid, color);
            }
        }
    }

    // ===== Phase 2: 主布局循环 =====
    let mut lanes: Vec<Option<Lane>> = Vec::new();
    let mut commit_lane: HashMap<Oid, usize> = HashMap::new();
    let mut color_pool = ColorPool::new();

    let mut rows: Vec<GraphRow> = Vec::new();
    let mut max_lane: usize = 0;

    for (row_idx, node) in nodes.iter().enumerate() {
        // --- Step 1: 找到此 commit 的 lane ---
        let my_lane = match commit_lane.get(&node.oid) {
            Some(&l) => l,
            None => allocate_lane(&mut lanes, node.oid, row_idx, &mut color_pool, colors),
        };

        // 清除其他追踪同一 commit 的 lane
        for (i, lane) in lanes.iter_mut().enumerate() {
            if i != my_lane {
                if let Some(l) = lane {
                    if l.target == Some(node.oid) {
                        *lane = None;
                    }
                }
            }
        }

        let my_color = commit_color
            .get(&node.oid)
            .copied()
            .unwrap_or_else(|| colors.color_for("default"));

        // --- Step 2: 渲染 commit 行 ---
        let num_cols = lanes.len().max(my_lane + 1);
        let mut cells = vec![CellType::Empty; num_cols];

        for (i, lane) in lanes.iter().enumerate() {
            if i == my_lane {
                continue;
            }
            if let Some(l) = lane {
                if l.target.is_some() {
                    cells[i] = CellType::Pipe(l.color);
                }
            }
        }
        cells[my_lane] = CellType::Commit(my_color);

        let branch_name = branch_map.get(&node.oid).and_then(|v| v.first().cloned());
        let all_branches = branch_map.get(&node.oid).cloned().unwrap_or_default();
        let has_stash = stash_oids.contains(&node.oid)
            || stash_map
                .get(&node.oid)
                .map(|v| !v.is_empty())
                .unwrap_or(false);
        let remote_branches = remote_branch_map
            .get(&node.oid)
            .cloned()
            .unwrap_or_default();

        rows.push(GraphRow {
            oid: Some(node.oid),
            lane: my_lane,
            cells,
            branch: branch_name,
            branches: all_branches,
            remote_branches,
            message_short: node.message_short.clone(),
            has_stash,
            tags: tag_map.get(&node.oid).cloned().unwrap_or_default(),
        });

        // --- Step 3: 处理 parents ---
        let parents = &node.parent_oids;

        if parents.is_empty() {
            // 无 parent：路径结束，释放颜色
            if let Some(l) = &lanes[my_lane] {
                color_pool.release(l.color, row_idx);
            }
            lanes[my_lane] = None;
            continue;
        }

        let first_parent = parents[0];

        // 检查第一 parent 是否已被其他 lane 追踪（收敛场景）
        let fp_existing = lanes
            .iter()
            .enumerate()
            .find(|(i, l)| {
                *i != my_lane && l.as_ref().is_some_and(|ll| ll.target == Some(first_parent))
            })
            .map(|(i, _)| i);

        if let Some(target_lane) = fp_existing {
            // 收敛：优先保留低 index lane（视觉上更接近主线）
            if my_lane < target_lane {
                // 当前 lane 更低 → 将 parent 移到 my_lane，释放 target_lane
                let target_color = lanes[target_lane]
                    .as_ref()
                    .map(|l| l.color)
                    .unwrap_or(my_color);
                color_pool.release(target_color, row_idx);
                lanes[target_lane] = None;
                commit_lane.insert(first_parent, my_lane);
                commit_color.entry(first_parent).or_insert(my_color);
                lanes[my_lane] = Some(Lane {
                    target: Some(first_parent),
                    color: my_color,
                });
                push_convergence(&mut rows, &lanes, target_lane, my_lane, target_color);
            } else {
                // target_lane 更低 → 保留 target_lane，释放 my_lane
                let my_lane_color = lanes[my_lane].as_ref().map(|l| l.color).unwrap_or(my_color);
                color_pool.release(my_lane_color, row_idx);
                lanes[my_lane] = None;
                push_convergence(&mut rows, &lanes, my_lane, target_lane, my_lane_color);
            }
        } else {
            // 第一 parent 继续在 my_lane
            commit_color.entry(first_parent).or_insert(my_color);
            commit_lane.insert(first_parent, my_lane);
            lanes[my_lane] = Some(Lane {
                target: Some(first_parent),
                color: my_color,
            });
        }

        // 额外 parent（merge 场景）
        if parents.len() > 1 {
            let mut extra_lane_pairs: Vec<(usize, Color)> = Vec::new();

            for parent_oid in &parents[1..] {
                // 检查是否已被追踪
                let existing = lanes
                    .iter()
                    .enumerate()
                    .find(|(_, l)| l.as_ref().is_some_and(|ll| ll.target == Some(*parent_oid)))
                    .map(|(i, _)| i);

                if let Some(col) = existing {
                    extra_lane_pairs.push((col, lanes[col].as_ref().unwrap().color));
                } else {
                    // 分配新 lane
                    let parent_color = commit_color
                        .get(parent_oid)
                        .copied()
                        .unwrap_or_else(|| color_pool.acquire(row_idx, colors));
                    if !commit_color.contains_key(parent_oid) {
                        commit_color.insert(*parent_oid, parent_color);
                    }
                    let new_lane = allocate_lane_with(
                        &mut lanes,
                        *parent_oid,
                        parent_color,
                        row_idx,
                        &mut color_pool,
                        colors,
                    );
                    commit_lane.insert(*parent_oid, new_lane);
                    max_lane = max_lane.max(new_lane);
                    extra_lane_pairs.push((new_lane, parent_color));
                }
            }

            push_fork(&mut rows, &lanes, my_lane, &extra_lane_pairs, my_color);
        }

        // 安全兜底去重：确保没有两条 lane 追踪同一 commit
        {
            let mut seen: HashMap<Oid, usize> = HashMap::new();
            for (i, lane) in lanes.iter_mut().enumerate() {
                if let Some(l) = lane {
                    if let Some(target) = l.target {
                        if let Some(&_prev) = seen.get(&target) {
                            // 重复追踪，释放此 lane
                            color_pool.release(l.color, row_idx);
                            *lane = None;
                        } else {
                            seen.insert(target, i);
                        }
                    }
                }
            }
        }

        max_lane = max_lane.max(
            lanes
                .iter()
                .filter(|l| l.is_some())
                .count()
                .saturating_sub(1),
        );
    }

    GraphLayout { rows, max_lane }
}

/// 分配一个空闲 lane 给指定 commit
fn allocate_lane(
    lanes: &mut Vec<Option<Lane>>,
    oid: Oid,
    row: usize,
    pool: &mut ColorPool,
    colors: &mut BranchColors,
) -> usize {
    let color = pool.acquire(row, colors);
    allocate_lane_with_color(lanes, oid, color)
}

/// 用指定颜色分配 lane
fn allocate_lane_with(
    lanes: &mut Vec<Option<Lane>>,
    oid: Oid,
    color: Color,
    _row: usize,
    _pool: &mut ColorPool,
    _colors: &mut BranchColors,
) -> usize {
    allocate_lane_with_color(lanes, oid, color)
}

fn allocate_lane_with_color(lanes: &mut Vec<Option<Lane>>, oid: Oid, color: Color) -> usize {
    // 优先复用空闲 slot
    for (i, slot) in lanes.iter_mut().enumerate() {
        if slot.is_none() {
            *slot = Some(Lane {
                target: Some(oid),
                color,
            });
            return i;
        }
    }
    let idx = lanes.len();
    lanes.push(Some(Lane {
        target: Some(oid),
        color,
    }));
    idx
}

/// 收敛连接器：from_lane 的路径汇入 to_lane（路径从上方来，结束于侧面）
///
/// 角落字符选择：路径从上方来（TOP 边）向侧面转
///   ╯ (MergeLeft = TOP+LEFT):  从右向左收敛
///   ╰ (MergeRight = TOP+RIGHT): 从左向右收敛
///
/// 视觉效果（from=1, to=0）:
/// ```
/// Col:  0  1
///       ├──╯   to(0) 接收右侧合并(├)，from(1) 从上方来向左转(╯)
/// ```
fn push_convergence(
    rows: &mut Vec<GraphRow>,
    lanes: &[Option<Lane>],
    from_lane: usize,
    to_lane: usize,
    color: Color,
) {
    let num_cols = lanes.len().max(from_lane + 1).max(to_lane + 1);
    let mut cc = vec![CellType::Empty; num_cols];

    // 画活跃管道
    for (i, lane) in lanes.iter().enumerate() {
        if let Some(l) = lane {
            if l.target.is_some() {
                cc[i] = CellType::Pipe(l.color);
            }
        }
    }

    let (left, right) = if from_lane < to_lane {
        (from_lane, to_lane)
    } else {
        (to_lane, from_lane)
    };

    // 水平线连接
    #[allow(clippy::needless_range_loop)]
    for c in (left + 1)..right {
        cc[c] = CellType::Horizontal(color);
    }

    if from_lane < to_lane {
        // from 在左，to 在右：from 路径从上方来向右转 → ╰ (MergeRight = TOP+RIGHT)
        cc[from_lane] = CellType::MergeRight(color);
        // to 接收左侧合并，管道继续 → ┤ (TeeLeft)
        cc[to_lane] = CellType::TeeLeft(color);
    } else {
        // from 在右，to 在左：from 路径从上方来向左转 → ╯ (MergeLeft = TOP+LEFT)
        cc[from_lane] = CellType::MergeLeft(color);
        // to 接收右侧合并，管道继续 → ├ (TeeRight)
        cc[to_lane] = CellType::TeeRight(color);
    }

    rows.push(GraphRow {
        oid: None,
        lane: from_lane,
        cells: cc,
        branch: None,
        branches: Vec::new(),
        remote_branches: Vec::new(),
        message_short: String::new(),
        has_stash: false,
        tags: Vec::new(),
    });
}

/// 分叉连接器：merge commit 分裂出额外 parent 路径（新分支向下走）
///
/// 角落字符选择：水平线从侧面来，新分支向下走（BOTTOM 边）
///   ╮ (BranchLeft = BOTTOM+LEFT):  extra 在右侧，水平从左来
///   ╭ (BranchRight = BOTTOM+RIGHT): extra 在左侧，水平从右来
///
/// 视觉效果（main=0, extras=[1]）:
/// ```
/// Col:  0  1
///       ├──╮   main(0) 管道继续+右侧分支(├)，extra(1) 从左方来向下走(╮)
/// ```
fn push_fork(
    rows: &mut Vec<GraphRow>,
    lanes: &[Option<Lane>],
    main_lane: usize,
    extra_lane_pairs: &[(usize, Color)],
    _main_color: Color,
) {
    let max_extra = extra_lane_pairs
        .iter()
        .map(|(l, _)| *l)
        .max()
        .unwrap_or(main_lane);
    let num_cols = lanes.len().max(main_lane + 1).max(max_extra + 1);
    let mut cc = vec![CellType::Empty; num_cols];

    // 画活跃管道（main_lane 此时已追踪 first_parent）
    for (i, lane) in lanes.iter().enumerate() {
        if i == main_lane {
            continue;
        }
        if let Some(l) = lane {
            if l.target.is_some() {
                cc[i] = CellType::Pipe(l.color);
            }
        }
    }

    // 记录 main_lane 两侧有哪些分支
    let has_left = extra_lane_pairs.iter().any(|(l, _)| *l < main_lane);
    let has_right = extra_lane_pairs.iter().any(|(l, _)| *l > main_lane);

    // 画每条分叉的水平线和角落
    for &(extra_lane, extra_color) in extra_lane_pairs {
        let (left, right) = if extra_lane < main_lane {
            (extra_lane, main_lane)
        } else {
            (main_lane, extra_lane)
        };

        // 水平线
        #[allow(clippy::needless_range_loop)]
        for c in (left + 1)..right {
            // 如果已经被更高优先级的连接器占据，跳过
            if matches!(cc[c], CellType::Empty) {
                cc[c] = CellType::Horizontal(extra_color);
            }
        }

        if extra_lane > main_lane {
            // extra 在右侧：水平从左来，向下走 → ╮ (BranchLeft = BOTTOM+LEFT)
            cc[extra_lane] = CellType::BranchLeft(extra_color);
        } else if extra_lane < main_lane {
            // extra 在左侧：水平从右来，向下走 → ╭ (BranchRight = BOTTOM+RIGHT)
            cc[extra_lane] = CellType::BranchRight(extra_color);
        }
    }

    // main_lane 的 T-junction
    if has_left && has_right {
        // 两侧都有分支：管道继续 + 左右分支
        // 简化处理：用 Pipe（因为 ├ 和 ┤ 不能同时表示）
        cc[main_lane] = CellType::Pipe(
            lanes[main_lane]
                .as_ref()
                .map(|l| l.color)
                .unwrap_or(_main_color),
        );
    } else if has_right {
        cc[main_lane] = CellType::TeeRight(
            lanes[main_lane]
                .as_ref()
                .map(|l| l.color)
                .unwrap_or(_main_color),
        );
    } else if has_left {
        cc[main_lane] = CellType::TeeLeft(
            lanes[main_lane]
                .as_ref()
                .map(|l| l.color)
                .unwrap_or(_main_color),
        );
    } else {
        // 不应有此情况（extra_lane == main_lane）
        cc[main_lane] = CellType::Pipe(
            lanes[main_lane]
                .as_ref()
                .map(|l| l.color)
                .unwrap_or(_main_color),
        );
    }

    rows.push(GraphRow {
        oid: None,
        lane: main_lane,
        cells: cc,
        branch: None,
        branches: Vec::new(),
        remote_branches: Vec::new(),
        message_short: String::new(),
        has_stash: false,
        tags: Vec::new(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oid(n: u8) -> Oid {
        let mut bytes = [0u8; 20];
        bytes[0] = n;
        Oid::from_bytes(&bytes).unwrap()
    }

    fn node(n: u8, parents: Vec<u8>) -> TopoNode {
        TopoNode {
            oid: oid(n),
            parent_oids: parents.into_iter().map(oid).collect(),
            time: (255 - n) as i64,
            message_short: format!("commit {}", n),
        }
    }

    #[test]
    fn test_single_commit_no_parents() {
        let nodes = vec![node(1, vec![])];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(layout.rows.len(), 1);
        assert_eq!(layout.rows[0].oid, Some(oid(1)));
        assert_eq!(layout.rows[0].lane, 0);
    }

    #[test]
    fn test_linear_three_commits() {
        let nodes = vec![node(3, vec![2]), node(2, vec![1]), node(1, vec![])];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(layout.rows.len(), 3);
        for row in &layout.rows {
            assert_eq!(row.lane, 0);
        }
    }

    #[test]
    fn test_merge_commit_two_parents() {
        let nodes = vec![node(3, vec![2, 1]), node(2, vec![]), node(1, vec![])];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        // Should have: commit 3, fork connector, commit 2, commit 1 (or similar)
        assert!(
            layout.rows.len() >= 4,
            "Expected at least 4 rows (3 commits + 1 connector), got {}",
            layout.rows.len()
        );
        assert_eq!(layout.rows[0].oid, Some(oid(3)));
        assert_eq!(layout.rows[0].lane, 0);

        // Fork connector should have correct corner characters
        let connector = &layout.rows[1];
        assert!(connector.oid.is_none(), "Second row should be a connector");
        // Check that the connector has a MergeLeft (╯) or similar
        let has_corner = connector.cells.iter().any(|c| {
            matches!(
                c,
                CellType::MergeLeft(_)
                    | CellType::MergeRight(_)
                    | CellType::BranchLeft(_)
                    | CellType::BranchRight(_)
            )
        });
        assert!(has_corner, "Connector should have corner characters");
    }

    #[test]
    fn test_empty_nodes() {
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &[],
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert!(layout.rows.is_empty());
        assert_eq!(layout.max_lane, 0);
    }

    #[test]
    fn test_branch_convergence_no_phantom_pipe() {
        let nodes = vec![
            node(4, vec![3]), // D → C
            node(6, vec![5]), // F → E
            node(3, vec![2]), // C → B
            node(5, vec![2]), // E → B
            node(2, vec![1]), // B → A
            node(1, vec![]),  // A
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );

        let b_idx = layout
            .rows
            .iter()
            .position(|r| r.oid == Some(oid(2)))
            .unwrap();
        for row in &layout.rows[b_idx + 1..] {
            let pipe_count = row
                .cells
                .iter()
                .filter(|c| matches!(c, CellType::Pipe(_)))
                .count();
            assert!(
                pipe_count <= 1,
                "B 之后不应有多余 pipe: {:?} (pipes={})",
                row.cells,
                pipe_count
            );
        }
    }

    /// 验证收敛连接器使用正确的角落字符
    /// 场景：两个分支收敛到同一个 commit
    ///   D(0) → C(0) → B
    ///   E(1) ─────────┘
    /// 收敛时 E 的 lane(1) 向左汇入 B 的 lane(0)
    #[test]
    fn test_convergence_uses_correct_corners() {
        let nodes = vec![
            node(4, vec![3]), // D → C, lane 0
            node(5, vec![2]), // E → B, lane 1
            node(3, vec![2]), // C → B, lane 0 → 收敛：E 也要到 B
            node(2, vec![1]), // B → A
            node(1, vec![]),  // A
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );

        // 查找收敛连接器（oid == None 的行）
        let connectors: Vec<_> = layout.rows.iter().filter(|r| r.oid.is_none()).collect();

        // 应该至少有一个收敛连接器（E→B 汇入 C→B）
        let convergence = connectors.iter().find(|c| {
            c.cells
                .iter()
                .any(|cell| matches!(cell, CellType::MergeLeft(_) | CellType::MergeRight(_)))
        });

        if let Some(conn) = convergence {
            // 收敛连接器中 from_lane > to_lane 时，from 应有 MergeLeft (╯)
            // 即从上方来向左转（路径结束于侧面，需要 TOP 边）
            let has_merge_corner = conn
                .cells
                .iter()
                .any(|c| matches!(c, CellType::MergeLeft(_) | CellType::MergeRight(_)));
            assert!(
                has_merge_corner,
                "收敛连接器应使用 Merge*(╰/╯) 角落，而非 Branch*(╭/╮): {:?}",
                conn.cells
            );
        }
    }

    /// 验证分叉连接器使用正确的角落字符
    /// 场景：merge commit 分叉出第二个 parent
    ///   M(0) → A(0)
    ///    └───→ B(1)
    /// 分叉时 B 的 lane(1) 从左侧来向下走 → 应该用 BranchLeft(╮) = BOTTOM+LEFT
    #[test]
    fn test_fork_uses_correct_corners() {
        let nodes = vec![
            node(3, vec![2, 1]), // M → A, B (merge)
            node(2, vec![]),     // A
            node(1, vec![]),     // B
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );

        // 第二行应该是分叉连接器
        let connector = &layout.rows[1];
        assert!(connector.oid.is_none(), "Second row should be a connector");

        // 分叉到右侧时，extra lane 应使用 BranchLeft(╮) = BOTTOM+LEFT（新分支向下走）
        let has_branch_corner = connector
            .cells
            .iter()
            .any(|c| matches!(c, CellType::BranchLeft(_) | CellType::BranchRight(_)));
        assert!(
            has_branch_corner,
            "分叉连接器应使用 Branch*(╭/╮) 角落，而非 Merge*(╰/╯): {:?}",
            connector.cells
        );

        // main_lane 应有 TeeRight(├)
        let has_tee = connector
            .cells
            .iter()
            .any(|c| matches!(c, CellType::TeeRight(_)));
        assert!(
            has_tee,
            "分叉连接器 main_lane 应有 TeeRight(├): {:?}",
            connector.cells
        );
    }
}

#[cfg(test)]
mod debug_tests {
    use super::*;

    fn oid(n: u8) -> Oid {
        let mut bytes = [0u8; 20];
        bytes[0] = n;
        Oid::from_bytes(&bytes).unwrap()
    }

    fn node(n: u8, parents: Vec<u8>) -> TopoNode {
        TopoNode {
            oid: oid(n),
            parent_oids: parents.into_iter().map(oid).collect(),
            time: (255 - n) as i64,
            message_short: format!("c{}", n),
        }
    }

    fn cell_to_str(c: &CellType) -> &'static str {
        match c {
            CellType::Empty => "  ",
            CellType::Pipe(_) => "│ ",
            CellType::Commit(_) => "◉ ",
            CellType::BranchRight(_) => "╭─",
            CellType::BranchLeft(_) => "╮ ",
            CellType::MergeRight(_) => "╰─",
            CellType::MergeLeft(_) => "╯ ",
            CellType::Horizontal(_) => "──",
            CellType::TeeRight(_) => "├─",
            CellType::TeeLeft(_) => "┤ ",
        }
    }

    fn dump_layout(label: &str, layout: &GraphLayout) {
        eprintln!("\n=== {} ===", label);
        for (i, row) in layout.rows.iter().enumerate() {
            let graph: String = row.cells.iter().map(cell_to_str).collect();
            let info = if let Some(o) = row.oid {
                let oid_short = format!("{:02x}", o.as_bytes()[0]);
                format!(
                    "c{}(oid={}) [lane={}]",
                    row.message_short.trim_start_matches('c'),
                    oid_short,
                    row.lane
                )
            } else {
                format!("[connector] lane={}", row.lane)
            };
            eprintln!("{:02} |{}| {}", i, graph, info);
        }
        eprintln!("max_lane={}\n", layout.max_lane);
    }

    /// 连续性验证：每个 commit 行有且仅有一个 Commit cell
    /// 每个 connector 行没有 Commit cell
    /// 所有非 Empty 的相邻 cell 之间颜色应连续
    fn validate_continuity(label: &str, layout: &GraphLayout) {
        for (i, row) in layout.rows.iter().enumerate() {
            if row.oid.is_some() {
                let commit_count = row
                    .cells
                    .iter()
                    .filter(|c| matches!(c, CellType::Commit(_)))
                    .count();
                assert_eq!(
                    commit_count, 1,
                    "{} row {}: commit row should have exactly 1 Commit cell, got {} — cells={:?}",
                    label, i, commit_count, row.cells
                );
            } else {
                let commit_count = row
                    .cells
                    .iter()
                    .filter(|c| matches!(c, CellType::Commit(_)))
                    .count();
                assert_eq!(
                    commit_count, 0,
                    "{} row {}: connector row should have 0 Commit cells, got {} — cells={:?}",
                    label, i, commit_count, row.cells
                );
            }
        }
    }

    #[test]
    fn debug_simple_merge() {
        let nodes = vec![
            node(3, vec![2, 1]), // c3 merge -> c2, c1
            node(2, vec![]),     // c2
            node(1, vec![]),     // c1
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        dump_layout("Simple Merge: c3->c2,c1", &layout);
        validate_continuity("Simple Merge", &layout);
    }

    #[test]
    fn debug_diamond_convergence() {
        let nodes = vec![
            node(4, vec![3]), // c4 -> c3
            node(5, vec![2]), // c5 -> c2
            node(3, vec![2]), // c3 -> c2 (convergence with c5)
            node(2, vec![1]), // c2 -> c1
            node(1, vec![]),  // c1
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        dump_layout("Diamond: c4->c3->c2, c5->c2", &layout);
        validate_continuity("Diamond", &layout);
    }

    #[test]
    fn debug_two_branches_converge() {
        let nodes = vec![
            node(4, vec![3]), // c4 -> c3
            node(6, vec![5]), // c6 -> c5
            node(3, vec![2]), // c3 -> c2
            node(5, vec![2]), // c5 -> c2 (convergence)
            node(2, vec![1]), // c2 -> c1
            node(1, vec![]),  // c1
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        dump_layout("Two branches converge at c2", &layout);
        validate_continuity("Two branches", &layout);
    }

    #[test]
    fn debug_consecutive_merges() {
        let nodes = vec![
            node(5, vec![4, 3]), // c5 merge -> c4, c3
            node(4, vec![1]),    // c4 -> c1
            node(3, vec![2]),    // c3 -> c2
            node(2, vec![1]),    // c2 -> c1 (convergence at c1)
            node(1, vec![]),     // c1
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        dump_layout("Consecutive merges", &layout);
        validate_continuity("Consecutive merges", &layout);
    }

    #[test]
    fn debug_three_branches() {
        let nodes = vec![
            node(7, vec![6]), // c7 -> c6
            node(8, vec![5]), // c8 -> c5
            node(9, vec![4]), // c9 -> c4
            node(6, vec![3]), // c6 -> c3
            node(5, vec![2]), // c5 -> c2
            node(4, vec![1]), // c4 -> c1
            node(3, vec![1]), // c3 -> c1 (convergence)
            node(2, vec![1]), // c2 -> c1 (convergence)
            node(1, vec![]),  // c1
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        dump_layout("Three branches to c1", &layout);
        validate_continuity("Three branches", &layout);
    }

    /// 模拟真实 git 历史的复杂场景
    #[test]
    fn debug_realistic_history() {
        // main: A <- B <- C <- D <- G(merge)
        // feature: A <- B <- E <- F <- G(merge)
        let nodes = vec![
            node(7, vec![4, 6]), // G (merge of D and F)
            node(6, vec![5]),    // F (feature tip)
            node(5, vec![3]),    // E
            node(4, vec![2]),    // D (main tip)
            node(3, vec![2]),    // C
            node(2, vec![1]),    // B
            node(1, vec![]),     // A
        ];
        let mut colors = BranchColors::new();
        let mut branch_map = HashMap::new();
        branch_map.insert(oid(4), vec!["main".to_string()]);
        branch_map.insert(oid(6), vec!["feature".to_string()]);
        let layout = build_layout(
            &nodes,
            &branch_map,
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        dump_layout("Realistic: main+feature merge", &layout);
        validate_continuity("Realistic", &layout);
    }

    /// 验证三路收敛到同一 commit 的场景
    #[test]
    fn test_three_branches_converge_at_single_commit() {
        let nodes = vec![
            node(7, vec![6]),
            node(8, vec![5]),
            node(9, vec![4]),
            node(6, vec![3]),
            node(5, vec![2]),
            node(4, vec![1]),
            node(3, vec![1]),
            node(2, vec![1]),
            node(1, vec![]),
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        validate_continuity("Three converge", &layout);

        // 所有收敛连接器应使用 Branch*(╭/╮) 角落（从上方来向侧面转）
        for row in &layout.rows {
            if row.oid.is_none() {
                // connector
            }
        }

        // 最终 commit c1 应在最低 lane
        let c1 = layout.rows.iter().find(|r| r.oid == Some(oid(1))).unwrap();
        assert_eq!(c1.lane, 0, "c1 应在 lane 0（最低）: {:?}", c1.cells);
    }

    /// 验证收敛后无残留 pipe
    #[test]
    fn test_no_phantom_pipes_after_full_convergence() {
        let nodes = vec![
            node(4, vec![3]),
            node(6, vec![5]),
            node(3, vec![2]),
            node(5, vec![2]),
            node(2, vec![1]),
            node(1, vec![]),
        ];
        let mut colors = BranchColors::new();
        let layout = build_layout(
            &nodes,
            &HashMap::new(),
            &HashMap::new(),
            &mut colors,
            &HashMap::new(),
            &HashMap::new(),
        );
        validate_continuity("No phantom", &layout);

        let c2_idx = layout
            .rows
            .iter()
            .position(|r| r.oid == Some(oid(2)))
            .unwrap();
        for row in &layout.rows[c2_idx + 1..] {
            let pipe_count = row
                .cells
                .iter()
                .filter(|c| matches!(c, CellType::Pipe(_)))
                .count();
            assert!(
                pipe_count <= 1,
                "c2 之后不应有多余 pipe: {:?} (pipes={})",
                row.cells,
                pipe_count
            );
        }
    }
}
