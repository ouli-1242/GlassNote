use serde::{Deserialize, Serialize};

// ─────────────────────────────── 时间约定 ───────────────────────────────
// 所有时间字段都是 Unix 毫秒（i64, UTC 纪元）。
// 存毫秒而不是字符串：范围查询与索引都走整数比较，比 ISO 字符串快且无格式歧义。
// 需要"本地墙钟"语义的地方（每天 00:00 出现、每月同一天）在 recurrence.rs 里
// 用 chrono::Local 换算，绝不在存储层做时区假设。

// ─────────────────────────────── 任务 ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub note: Option<String>,
    /// todo | done
    pub status: String,
    /// 0=无 1=低 2=普通 3=高 4=紧急
    pub priority: i64,
    pub tags: Vec<String>,
    /// 出现/截止时间
    pub due_at: Option<i64>,
    /// 提醒时间，可独立于 due_at
    pub remind_at: Option<i64>,
    /// once | daily | weekly | monthly | weekday | interval | cron
    pub repeat_type: String,
    /// interval 类型下的 N
    pub repeat_interval: i64,
    /// interval 类型下的单位：day | week | month
    pub repeat_unit: String,
    /// ISO 星期（1=周一 … 7=周日），weekly 用
    pub repeat_weekdays: Vec<i64>,
    /// monthly 用，1..31；-1 表示沿用 due_at 的日
    pub repeat_monthday: Option<i64>,
    /// 重复结束时间，超过则不再产生下一次
    pub repeat_end_at: Option<i64>,
    /// cron 表达式（5 段）
    pub repeat_cron: Option<String>,
    /// 子任务指向父任务
    pub parent_id: Option<i64>,
    pub pinned: bool,
    pub sort_order: f64,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
    /// 非空即在回收站
    pub deleted_at: Option<i64>,
    /// 本实例的提醒是否已发出；重复任务推进到下一次时清空
    pub remind_fired_at: Option<i64>,
}

/// 新建 / 编辑任务的入参。id 为空表示新建。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub id: Option<i64>,
    pub title: String,
    pub note: Option<String>,
    pub priority: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub due_at: Option<i64>,
    pub remind_at: Option<i64>,
    pub repeat_type: Option<String>,
    pub repeat_interval: Option<i64>,
    pub repeat_unit: Option<String>,
    pub repeat_weekdays: Option<Vec<i64>>,
    pub repeat_monthday: Option<i64>,
    pub repeat_end_at: Option<i64>,
    pub repeat_cron: Option<String>,
    pub parent_id: Option<i64>,
}

/// 任务列表查询条件
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskQuery {
    /// today | all | done | trash | upcoming
    pub view: Option<String>,
    pub search: Option<String>,
    pub tag: Option<String>,
    pub priority: Option<i64>,
    /// due | priority | created | manual
    pub sort: Option<String>,
}

// ─────────────────────────────── 备忘录 ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Memo {
    pub id: i64,
    pub title: String,
    pub content: String,
    /// 主题色索引，前端映射成实际色值
    pub color: String,
    pub collapsed: bool,
    pub pinned: bool,
    pub sort_order: f64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoInput {
    pub id: Option<i64>,
    pub title: String,
    pub content: Option<String>,
    pub color: Option<String>,
    pub collapsed: Option<bool>,
    pub pinned: Option<bool>,
}

// ─────────────────────────────── 完成记录 / 例外 ───────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletedLog {
    pub id: i64,
    pub task_id: i64,
    pub task_title: String,
    pub completed_at: i64,
    /// complete | uncomplete | skip
    pub action: String,
    /// 完成前的 due_at，用于撤销时精确回滚重复任务
    pub prev_due_at: Option<i64>,
}

// ─────────────────────────────── 设置 ───────────────────────────────

/// 玻璃效果的探测结果，回传给前端决定要不要打开 CSS 模糊兜底。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlassInfo {
    /// 实际生效的效果：mica | tabbed | acrylic | blur | css
    pub effect: String,
    /// 由"哪个效果成功了"推导，而不是去读系统版本号。
    ///
    /// 理由：决定降级路径的是**系统实际支持哪个效果**，不是营销版本号。
    /// Windows 11 上用户关掉"透明效果"后 Mica/Acrylic 都会失败，
    /// 此时报"用的是 Acrylic 降级"才是对用户真实可见结果的准确描述。
    pub is_windows_11: bool,
    /// 是否已应用 DWM 圆角区域
    pub rounded: bool,
    /// 前端应当使用的面板圆角半径（px）。
    ///
    /// 由 Rust 决定而不是前端写死：它必须小于等于 DWM 的裁切半径，
    /// 这是系统几何约束，不是视觉偏好（详见 glass.rs 的说明）。
    pub corner_radius: f64,
}
