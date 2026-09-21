//! 重复规则引擎。
//!
//! 设计要点：**一条任务行代表一个"系列"，`due_at` 永远指向"下一次出现的时间"**。
//! 完成后不是新建一条记录，而是把 `due_at` 推进到下一次。这样：
//!   - "今天完成后今天不再出现、明天重新出现" 自然成立（due_at 已变成明天）；
//!   - 列表查询只需比较 due_at 与当前时间，不需要额外的"已完成实例"表；
//!   - 撤销完成只要把 due_at 写回旧值即可（旧值记在 completed_logs.prev_due_at）。
//!
//! 推进基准取 `max(now, due_at)`：逾期多天才完成时不会补出多条实例，
//! 而提前完成（due_at 在未来）时也不会把下一次拉回过去。

use chrono::{DateTime, Datelike, Duration, Local, LocalResult, NaiveDate, NaiveTime, TimeZone, Timelike};

use crate::models::Task;

/// 解析后的重复规则
#[derive(Debug, Clone, Default)]
pub struct Rule {
    /// once | daily | weekly | monthly | weekday | interval | cron
    pub repeat_type: String,
    pub interval: i64,
    /// day | week | month
    pub unit: String,
    /// ISO 星期 1..7
    pub weekdays: Vec<u32>,
    pub monthday: Option<i32>,
    pub end_at: Option<i64>,
    pub cron: Option<String>,
}

impl Rule {
    pub fn from_task(t: &Task) -> Self {
        Self {
            repeat_type: t.repeat_type.clone(),
            interval: t.repeat_interval.max(1),
            unit: t.repeat_unit.clone(),
            weekdays: t.repeat_weekdays.iter().filter_map(|d| u32::try_from(*d).ok()).collect(),
            monthday: t.repeat_monthday.and_then(|d| i32::try_from(d).ok()),
            end_at: t.repeat_end_at,
            cron: t.repeat_cron.clone(),
        }
    }

    /// 非一次性且规则可用时才叫"重复任务"
    pub fn is_repeating(&self) -> bool {
        match self.repeat_type.as_str() {
            "daily" | "weekly" | "monthly" | "weekday" => true,
            "interval" => self.interval >= 1,
            "cron" => self.cron.as_deref().map(|c| !c.trim().is_empty()).unwrap_or(false),
            _ => false,
        }
    }
}

/// 把本地日期+时刻转成 Unix 毫秒。
///
/// 夏令时下本地时刻可能不存在（跳变）或出现两次（重叠），必须显式处理：
///   - 不存在（如 02:30 被跳过）：顺延一小时取第一个合法时刻，保证任务不会凭空消失；
///   - 出现两次：取较早的一次，保证结果稳定、可重复。
fn local_ms(date: NaiveDate, time: NaiveTime) -> Option<i64> {
    let ndt = date.and_time(time);
    match Local.from_local_datetime(&ndt) {
        LocalResult::Single(dt) => Some(dt.timestamp_millis()),
        LocalResult::Ambiguous(earlier, _) => Some(earlier.timestamp_millis()),
        LocalResult::None => {
            let shifted = ndt + Duration::hours(1);
            Local
                .from_local_datetime(&shifted)
                .earliest()
                .map(|dt| dt.timestamp_millis())
        }
    }
}

fn to_local(ms: i64) -> Option<DateTime<Local>> {
    Local.timestamp_millis_opt(ms).single()
}

/// 计算 `from_ms` **之后**（严格大于）的下一次出现时间；无下一次返回 None。
pub fn next_after(rule: &Rule, from_ms: i64, seed_ms: i64) -> Option<i64> {
    if !rule.is_repeating() {
        return None;
    }
    let seed = to_local(seed_ms)?;
    let from = to_local(from_ms)?;
    let time = seed.time();
    let anchor = seed.date_naive();

    let candidate = match rule.repeat_type.as_str() {
        "daily" => nth_day(anchor, time, 1, from),
        "weekday" => nth_business_day(anchor, time, from),
        "weekly" => nth_week(anchor, time, &rule.weekdays, 1, from),
        "monthly" => nth_month(anchor, time, rule.monthday, 1, from),
        "interval" => match rule.unit.as_str() {
            "week" => nth_week(anchor, time, &rule.weekdays, rule.interval, from),
            "month" => nth_month(anchor, time, rule.monthday, rule.interval, from),
            _ => nth_day(anchor, time, rule.interval, from),
        },
        "cron" => rule.cron.as_deref().and_then(|c| cron_next(c, from)),
        _ => None,
    };

    match (candidate, rule.end_at) {
        // 重复结束日期之后不再产生实例，整条序列就此结束
        (Some(c), Some(end)) if c > end => None,
        (c, _) => c,
    }
}

/// 每隔 step 天的序列，锚点是 anchor。
///
/// 用 `delta / step` 直接跳到 from 附近，而不是从锚点线性推进 ——
/// 一个逾期半年的每日任务如果线性推进要循环 180 次，这里只算一次除法。
fn nth_day(anchor: NaiveDate, time: NaiveTime, step: i64, from: DateTime<Local>) -> Option<i64> {
    let step = step.max(1);
    let delta = (from.date_naive() - anchor).num_days();
    let mut k = if delta <= 0 { 0 } else { delta / step };
    // 允许几轮修正，覆盖"候选时刻早于 from 但日期相同"这类边界
    for _ in 0..4 {
        let date = anchor.checked_add_signed(Duration::days(k * step))?;
        if let Some(ms) = local_ms(date, time) {
            if ms > from.timestamp_millis() {
                return Some(ms);
            }
        }
        k += 1;
    }
    None
}

/// 工作日序列：周一到周五，逐日推进。
fn nth_business_day(anchor: NaiveDate, time: NaiveTime, from: DateTime<Local>) -> Option<i64> {
    let delta = (from.date_naive() - anchor).num_days();
    let mut k = delta.max(0);
    // 最多看 14 天，一定覆盖"跨一个周末 + 当天时刻已过"的情况
    for _ in 0..14 {
        let date = anchor.checked_add_signed(Duration::days(k))?;
        if !matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
            if let Some(ms) = local_ms(date, time) {
                if ms > from.timestamp_millis() {
                    return Some(ms);
                }
            }
        }
        k += 1;
    }
    None
}

/// 按周推进的序列。weekdays 为空时沿用锚点当天的星期。
fn nth_week(
    anchor: NaiveDate,
    time: NaiveTime,
    weekdays: &[u32],
    step_weeks: i64,
    from: DateTime<Local>,
) -> Option<i64> {
    let step = step_weeks.max(1);
    let mut days: Vec<u32> = if weekdays.is_empty() {
        vec![anchor.weekday().number_from_monday()]
    } else {
        weekdays.to_vec()
    };
    days.sort_unstable();
    days.dedup();

    // 以锚点所在周的周一为基准，所有候选日期都能表示为 monday + week*7 + (dow-1)
    let monday = anchor - Duration::days(i64::from(anchor.weekday().number_from_monday()) - 1);
    let delta_days = (from.date_naive() - monday).num_days();
    let mut week = if delta_days <= 0 { 0 } else { delta_days / (7 * step) };

    // 最多看 8 个周期，足以越过"本周所有候选都已过"的情况
    for _ in 0..8 {
        for d in &days {
            let offset = week * step * 7 + i64::from(*d) - 1;
            let date = monday.checked_add_signed(Duration::days(offset))?;
            if let Some(ms) = local_ms(date, time) {
                if ms > from.timestamp_millis() {
                    return Some(ms);
                }
            }
        }
        week += 1;
    }
    None
}

/// 按月推进的序列。monthday 为 None 或 -1 时沿用锚点的日，短月自动收敛到月末。
fn nth_month(
    anchor: NaiveDate,
    time: NaiveTime,
    monthday: Option<i32>,
    step_months: i64,
    from: DateTime<Local>,
) -> Option<i64> {
    let step = step_months.max(1);
    let want_day = match monthday {
        Some(d) if d >= 1 => d as u32,
        // -1 或未设置：沿用锚点当天，这样"每月 15 号"不需要用户额外配置
        _ => anchor.day(),
    };

    let month_index = |d: NaiveDate| -> i64 { i64::from(d.year()) * 12 + i64::from(d.month0()) };
    let anchor_idx = month_index(anchor);
    let from_idx = month_index(from.date_naive());
    let mut k = if from_idx <= anchor_idx {
        0
    } else {
        (from_idx - anchor_idx) / step
    };

    for _ in 0..4 {
        let total = anchor_idx + k * step;
        let year = (total.div_euclid(12)) as i32;
        let month0 = total.rem_euclid(12) as u32;
        // 31 号在 2 月不存在，收敛到该月最后一天
        let last = days_in_month(year, month0 + 1);
        let day = want_day.min(last);
        if let Some(date) = NaiveDate::from_ymd_opt(year, month0 + 1, day) {
            if let Some(ms) = local_ms(date, time) {
                if ms > from.timestamp_millis() {
                    return Some(ms);
                }
            }
        }
        k += 1;
    }
    None
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let first = NaiveDate::from_ymd_opt(year, month, 1);
    let next = NaiveDate::from_ymd_opt(ny, nm, 1);
    match (first, next) {
        (Some(a), Some(b)) => (b - a).num_days() as u32,
        _ => 30,
    }
}

// ─────────────────────────────── Cron ───────────────────────────────
// 只支持标准 5 段：分 时 日 月 周。支持 * / a / a-b / */n / a-b/n / 逗号列表。
// 不做秒级与 @macro —— 便签场景用不到，支持它们只会让解析器更难验证。

#[derive(Debug, Clone)]
struct CronField {
    /// 允许的取值集合（已展开）
    allowed: Vec<u32>,
}

fn parse_field(raw: &str, min: u32, max: u32) -> Option<CronField> {
    let mut allowed = Vec::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        let (range_part, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().ok()?.max(1)),
            None => (part, 1),
        };
        let (lo, hi) = if range_part == "*" {
            (min, max)
        } else if let Some((a, b)) = range_part.split_once('-') {
            (a.trim().parse().ok()?, b.trim().parse().ok()?)
        } else {
            let v: u32 = range_part.parse().ok()?;
            (v, v)
        };
        if lo < min || hi > max || lo > hi {
            return None;
        }
        let mut v = lo;
        while v <= hi {
            allowed.push(v);
            v += step;
        }
    }
    allowed.sort_unstable();
    allowed.dedup();
    if allowed.is_empty() {
        None
    } else {
        Some(CronField { allowed })
    }
}

/// 在 from 之后找下一个匹配的分钟。最多向前搜 2 年，超出返回 None。
fn cron_next(expr: &str, from: DateTime<Local>) -> Option<i64> {
    let parts: Vec<&str> = expr.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }
    let minute = parse_field(parts[0], 0, 59)?;
    let hour = parse_field(parts[1], 0, 23)?;
    let dom = parse_field(parts[2], 1, 31)?;
    let month = parse_field(parts[3], 1, 12)?;
    let dow = parse_field(parts[4], 0, 6)?; // 0=周日

    let dom_restricted = parts[2] != "*";
    let dow_restricted = parts[4] != "*";

    // 从下一分钟开始，保证严格大于 from
    let mut t = (from + Duration::minutes(1)).with_second(0)?.with_nanosecond(0)?;
    let limit = t + Duration::days(731);

    while t <= limit {
        let wd = t.weekday().num_days_from_sunday();
        let day_ok = if dom_restricted && dow_restricted {
            // 标准 cron 语义：日与周同时受限时取"或"
            dom.allowed.contains(&t.day()) || dow.allowed.contains(&wd)
        } else if dom_restricted {
            dom.allowed.contains(&t.day())
        } else if dow_restricted {
            dow.allowed.contains(&wd)
        } else {
            true
        };

        if month.allowed.contains(&t.month()) && day_ok {
            if hour.allowed.contains(&t.hour()) && minute.allowed.contains(&t.minute()) {
                return Some(t.timestamp_millis());
            }
            // 当天还有戏：跳到下一个允许的小时
            t += Duration::minutes(1);
        } else {
            // 整天不匹配：直接跳到次日 00:00，避免逐分钟空转
            let next_day = t.date_naive().checked_add_signed(Duration::days(1))?;
            t = Local
                .from_local_datetime(&next_day.and_hms_opt(0, 0, 0)?)
                .earliest()?;
        }
    }
    None
}

/// 把重复规则描述成中文，供 UI 直接展示（前端不重复实现一遍规则语义）。
pub fn describe(rule: &Rule) -> String {
    match rule.repeat_type.as_str() {
        "daily" => "每天".into(),
        "weekday" => "每个工作日".into(),
        "weekly" => {
            if rule.weekdays.is_empty() {
                "每周".into()
            } else {
                let names = ["一", "二", "三", "四", "五", "六", "日"];
                let list: Vec<String> = rule
                    .weekdays
                    .iter()
                    .filter(|d| (1..=7).contains(*d))
                    .map(|d| format!("周{}", names[(*d - 1) as usize]))
                    .collect();
                format!("每{}", list.join("、"))
            }
        }
        "monthly" => match rule.monthday {
            Some(d) if d >= 1 => format!("每月 {d} 日"),
            _ => "每月".into(),
        },
        "interval" => {
            let unit = match rule.unit.as_str() {
                "week" => "周",
                "month" => "月",
                _ => "天",
            };
            format!("每 {} {}", rule.interval, unit)
        }
        "cron" => rule
            .cron
            .as_deref()
            .map(|c| format!("Cron：{c}"))
            .unwrap_or_else(|| "Cron".into()),
        _ => "不重复".into(),
    }
}

#[cfg(test)]
mod tests {
    //! 为什么这里保留断言，而不是"最宽泛的检查"：
    //!
    //! 日期推进属于**错了不会报错**的一类 —— `next_after` 算错一天不会抛异常、不会编译失败，
    //! 表现只是任务在某天莫名不出现。这类静默错误只能靠精确断言兜住，
    //! 属于"破了会静默出错"的范畴，因此值得留。
    //!
    //! 断言的是**规则语义**（每月 31 号在 2 月收敛到 28 号、逾期不补实例、结束日期终止序列），
    //! 不是实现细节：把内部从 chrono 换成别的时间库，这些断言依然成立。
    //! 验收标准仍然是 `cargo build` + 真实使用，不是测试数量。

    use super::*;

    fn at(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> i64 {
        Local
            .from_local_datetime(&NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(hh, mm, 0).unwrap())
            .earliest()
            .unwrap()
            .timestamp_millis()
    }

    fn fmt(ms: i64) -> String {
        to_local(ms).unwrap().format("%Y-%m-%d %H:%M").to_string()
    }

    fn rule(t: &str) -> Rule {
        Rule {
            repeat_type: t.into(),
            interval: 1,
            unit: "day".into(),
            ..Default::default()
        }
    }

    #[test]
    fn daily_advances_to_next_day_same_time() {
        let seed = at(2026, 3, 10, 9, 0);
        let next = next_after(&rule("daily"), seed, seed).unwrap();
        assert_eq!(fmt(next), "2026-03-11 09:00");
    }

    #[test]
    fn daily_overdue_by_months_does_not_catch_up() {
        // 逾期 90 天才完成，下一次只应是"明天"，而不是补出 90 条
        let seed = at(2026, 1, 1, 8, 30);
        let now = at(2026, 4, 1, 20, 0);
        let next = next_after(&rule("daily"), now, seed).unwrap();
        assert_eq!(fmt(next), "2026-04-02 08:30");
    }

    #[test]
    fn monthly_clamps_to_short_month() {
        let mut r = rule("monthly");
        r.monthday = Some(31);
        let seed = at(2026, 1, 31, 10, 0);
        // 2 月没有 31 号，应收敛到 2 月最后一天
        assert_eq!(fmt(next_after(&r, seed, seed).unwrap()), "2026-02-28 10:00");
    }

    #[test]
    fn monthly_crosses_year_boundary() {
        let mut r = rule("monthly");
        r.monthday = Some(15);
        let seed = at(2026, 12, 15, 10, 0);
        assert_eq!(fmt(next_after(&r, seed, seed).unwrap()), "2027-01-15 10:00");
    }

    #[test]
    fn weekly_picks_next_listed_weekday() {
        let mut r = rule("weekly");
        r.weekdays = vec![1, 3, 5]; // 一、三、五
        // 2026-03-11 是周三，下一次应是周五 03-13
        let seed = at(2026, 3, 11, 9, 0);
        assert_eq!(fmt(next_after(&r, seed, seed).unwrap()), "2026-03-13 09:00");
    }

    #[test]
    fn weekly_wraps_to_next_week() {
        let mut r = rule("weekly");
        r.weekdays = vec![1, 3, 5];
        let seed = at(2026, 3, 13, 9, 0); // 周五
        assert_eq!(fmt(next_after(&r, seed, seed).unwrap()), "2026-03-16 09:00"); // 下周一
    }

    #[test]
    fn weekday_skips_weekend() {
        let seed = at(2026, 3, 13, 9, 0); // 周五
        assert_eq!(fmt(next_after(&rule("weekday"), seed, seed).unwrap()), "2026-03-16 09:00");
    }

    #[test]
    fn interval_days_anchors_on_seed_not_on_now() {
        let mut r = rule("interval");
        r.interval = 3;
        let seed = at(2026, 3, 1, 7, 0);
        // 锚点是 3/1，候选为 3/4、3/7、3/10…，从 3/5 往后第一次是 3/7
        assert_eq!(fmt(next_after(&r, at(2026, 3, 5, 12, 0), seed).unwrap()), "2026-03-07 07:00");
    }

    #[test]
    fn interval_weeks() {
        let mut r = rule("interval");
        r.unit = "week".into();
        r.interval = 2;
        let seed = at(2026, 3, 2, 9, 0); // 周一
        assert_eq!(fmt(next_after(&r, seed, seed).unwrap()), "2026-03-16 09:00");
    }

    #[test]
    fn repeat_end_at_stops_series() {
        let mut r = rule("daily");
        let seed = at(2026, 3, 10, 9, 0);
        r.end_at = Some(at(2026, 3, 11, 0, 0));
        assert_eq!(next_after(&r, seed, seed), None);
    }

    #[test]
    fn once_has_no_next() {
        assert_eq!(next_after(&rule("once"), 0, 0), None);
    }

    #[test]
    fn cron_every_minute_step() {
        let mut r = rule("cron");
        r.cron = Some("*/15 * * * *".into());
        let from = at(2026, 3, 10, 9, 7);
        assert_eq!(fmt(next_after(&r, from, from).unwrap()), "2026-03-10 09:15");
    }

    #[test]
    fn cron_weekday_morning() {
        let mut r = rule("cron");
        r.cron = Some("30 8 * * 1-5".into());
        let from = at(2026, 3, 13, 9, 0); // 周五 9:00，当天 8:30 已过
        assert_eq!(fmt(next_after(&r, from, from).unwrap()), "2026-03-16 08:30"); // 下周一
    }

    #[test]
    fn cron_rejects_malformed() {
        let mut r = rule("cron");
        r.cron = Some("not a cron".into());
        assert_eq!(next_after(&r, at(2026, 3, 10, 9, 0), at(2026, 3, 10, 9, 0)), None);
    }

    #[test]
    fn monthday_minus_one_follows_anchor_day() {
        let mut r = rule("monthly");
        r.monthday = Some(-1);
        let seed = at(2026, 1, 17, 6, 0);
        assert_eq!(fmt(next_after(&r, seed, seed).unwrap()), "2026-02-17 06:00");
    }
}
