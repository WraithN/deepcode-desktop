//! 通用静默看门狗：区分"agent 忙但活着"与"进程真卡死"，插件无关。
//!
//! 设计背景：LLM 单次长生成（如 comet classic 大上下文流程）可能超过 10 分钟
//! 不产生任何 SSE 事件，这属于正常现象；若按"静默即杀"处理，在途生成内容作废、
//! 进程重启后历史重放还会导致同一工具被重复执行，形成死循环。因此静默超窗后
//! 先调用插件的活性探针：探活成功说明 agent 仍在工作，继续等待；探活失败才
//! 判定卡死。另有静默硬上限兜底，防止"LLM SDK 永久挂死但探活仍成功"时 run
//! 永不结束。

use std::sync::Mutex;
use std::time::Duration;
use tokio::time::Instant;

/// 看门狗周期性检查活跃度的时间间隔（秒）。
pub const WATCHDOG_CHECK_INTERVAL_SECS: u64 = 10;
/// 单次活性探针的超时（秒）：探针自身挂起按探活失败处理。
pub const PROBE_TIMEOUT_SECS: u64 = 5;
/// 静默总时长硬上限（秒）：无论探活结果如何，超过即判定卡死。
/// 取值约为实测最长 LLM 单次生成（约 20 分钟）的 3 倍，正常生成不会触及。
pub const MAX_SILENCE_CAP_SECS: u64 = 3600;

/// 卡死判定原因，供调用方生成错误消息与日志。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StallReason {
    /// 静默超窗且活性探针失败/超时——进程真卡死。
    ProbeFailed,
    /// 静默总时长超过硬上限（探活可能仍成功，但生成已不可能合理恢复）。
    SilenceCapExceeded,
}

/// 静默看门狗：周期性检查最近事件时间，直到判定 agent 卡死时返回。
///
/// 通常在 `tokio::select!` 中与 agent 请求 future 竞争；请求先完成则本
/// future 被 drop，无副作用。
///
/// 判定逻辑（每 `WATCHDOG_CHECK_INTERVAL_SECS` 检查一次）：
/// - `last_event_at` 为 None（尚无事件时间基准）→ 继续等待；
/// - 静默未超过 `silence_timeout_secs`（动态读取，支持运行时更新）→ 继续等待；
/// - 静默超窗但未达硬上限 → 调用 `probe` 探活：
///   - 成功 → 打 warn 日志（silent but alive）并继续等待；
///   - 失败或探针自身超时 → 返回 [`StallReason::ProbeFailed`]；
/// - 静默总时长超过 `MAX_SILENCE_CAP_SECS` → 返回 [`StallReason::SilenceCapExceeded`]。
///
/// `probe` 返回 true 表示 agent 存活；`log_tag` 为日志定位前缀（如插件名+实例 id）。
pub async fn wait_until_stalled<F, Fut>(
    last_event_at: &Mutex<Option<Instant>>,
    silence_timeout_secs: &Mutex<u64>,
    probe: F,
    log_tag: &str,
) -> StallReason
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let interval = Duration::from_secs(WATCHDOG_CHECK_INTERVAL_SECS);
    let cap = Duration::from_secs(MAX_SILENCE_CAP_SECS);
    loop {
        tokio::time::sleep(interval).await;
        let elapsed = match last_event_at.lock().unwrap().as_ref() {
            Some(t) => t.elapsed(),
            None => continue,
        };
        if elapsed > cap {
            log::warn!(
                "[watchdog] {log_tag} agent silent for {}s exceeding hard cap {}s, treating as stalled",
                elapsed.as_secs(),
                MAX_SILENCE_CAP_SECS
            );
            return StallReason::SilenceCapExceeded;
        }
        let configured = *silence_timeout_secs.lock().unwrap();
        if elapsed <= Duration::from_secs(configured) {
            continue;
        }
        let alive = probe_with_timeout(&probe).await;
        if !alive {
            log::warn!(
                "[watchdog] {log_tag} agent stalled: no events for {}s and liveness probe failed",
                elapsed.as_secs()
            );
            return StallReason::ProbeFailed;
        }
        log::warn!(
            "[watchdog] {log_tag} agent silent for {}s (window {}s) but liveness probe OK; keep waiting",
            elapsed.as_secs(),
            configured
        );
    }
}

/// 执行一次活性探针，自身挂起超过 `PROBE_TIMEOUT_SECS` 按失败处理。
async fn probe_with_timeout<F, Fut>(probe: &F) -> bool
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    match tokio::time::timeout(Duration::from_secs(PROBE_TIMEOUT_SECS), probe()).await {
        Ok(alive) => alive,
        Err(_) => {
            log::warn!("[watchdog] liveness probe timed out after {PROBE_TIMEOUT_SECS}s");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    const TEST_WINDOW_SECS: u64 = 30;

    fn fixture(window_secs: u64) -> (Arc<Mutex<Option<Instant>>>, Arc<Mutex<u64>>) {
        let last_event_at = Arc::new(Mutex::new(Some(Instant::now())));
        let window = Arc::new(Mutex::new(window_secs));
        (last_event_at, window)
    }

    #[tokio::test(start_paused = true)]
    async fn test_no_event_baseline_never_stalls() {
        // last_event_at 为 None：无判定基准，既不探活也不判死。
        let last_event_at = Arc::new(Mutex::new(None));
        let window = Arc::new(Mutex::new(TEST_WINDOW_SECS));
        let probe_calls = Arc::new(AtomicUsize::new(0));
        let calls = probe_calls.clone();
        let probe = move || {
            calls.fetch_add(1, Ordering::SeqCst);
            std::future::ready(false)
        };
        let result = tokio::time::timeout(
            Duration::from_secs(TEST_WINDOW_SECS * 4),
            wait_until_stalled(&last_event_at, &window, probe, "test"),
        )
        .await;
        assert!(result.is_err(), "无事件基准时不应判死");
        assert_eq!(probe_calls.load(Ordering::SeqCst), 0, "无事件基准时不应探活");
    }

    #[tokio::test(start_paused = true)]
    async fn test_within_window_no_probe_no_stall() {
        // 静默未超窗：不探活、不判死。
        let (last_event_at, window) = fixture(TEST_WINDOW_SECS);
        let probe_calls = Arc::new(AtomicUsize::new(0));
        let calls = probe_calls.clone();
        let probe = move || {
            calls.fetch_add(1, Ordering::SeqCst);
            std::future::ready(false)
        };
        let result = tokio::time::timeout(
            Duration::from_secs(TEST_WINDOW_SECS),
            wait_until_stalled(&last_event_at, &window, probe, "test"),
        )
        .await;
        assert!(result.is_err(), "静默未超窗时不应判死");
        assert_eq!(probe_calls.load(Ordering::SeqCst), 0, "静默未超窗时不应探活");
    }

    #[tokio::test(start_paused = true)]
    async fn test_probe_failure_after_window_returns_stalled() {
        // 静默超窗且探活失败：判定卡死。
        let (last_event_at, window) = fixture(TEST_WINDOW_SECS);
        let probe = || std::future::ready(false);
        let reason = wait_until_stalled(&last_event_at, &window, probe, "test").await;
        assert_eq!(reason, StallReason::ProbeFailed);
    }

    #[tokio::test(start_paused = true)]
    async fn test_probe_ok_keeps_waiting_then_failure_stalls() {
        // 探活成功期间不判死（LLM 长生成场景）；之后探活转失败则判死。
        let (last_event_at, window) = fixture(TEST_WINDOW_SECS);
        let alive = Arc::new(AtomicBool::new(true));
        let probe_calls = Arc::new(AtomicUsize::new(0));
        let flag = alive.clone();
        let calls = probe_calls.clone();
        let probe = move || {
            calls.fetch_add(1, Ordering::SeqCst);
            let v = flag.load(Ordering::SeqCst);
            std::future::ready(v)
        };
        let watchdog = tokio::spawn(wait_until_stalled_owned(
            last_event_at.clone(),
            window.clone(),
            probe,
        ));
        // 虚拟时间推进 3 倍窗口：看门狗应多次探活但不判死。
        tokio::time::sleep(Duration::from_secs(TEST_WINDOW_SECS * 3)).await;
        assert!(!watchdog.is_finished(), "探活成功时不应判死");
        assert!(
            probe_calls.load(Ordering::SeqCst) >= 2,
            "超窗后应周期性探活，实际 {} 次",
            probe_calls.load(Ordering::SeqCst)
        );
        // 探活转为失败：看门狗应在下一次检查时判死。
        alive.store(false, Ordering::SeqCst);
        let reason = tokio::time::timeout(Duration::from_secs(TEST_WINDOW_SECS), watchdog)
            .await
            .expect("探活失败后应判死")
            .unwrap();
        assert_eq!(reason, StallReason::ProbeFailed);
    }

    #[tokio::test(start_paused = true)]
    async fn test_silence_cap_exceeded_even_when_probe_ok() {
        // 探活持续成功但静默总时长超过硬上限：仍判死，防止 run 永不结束。
        let (last_event_at, window) = fixture(1);
        let probe = || std::future::ready(true);
        let reason = wait_until_stalled(&last_event_at, &window, probe, "test").await;
        assert_eq!(reason, StallReason::SilenceCapExceeded);
    }

    #[tokio::test(start_paused = true)]
    async fn test_hanging_probe_treated_as_failure() {
        // 探针自身挂起：超过 PROBE_TIMEOUT_SECS 按探活失败处理。
        let (last_event_at, window) = fixture(TEST_WINDOW_SECS);
        let probe = || std::future::pending::<bool>();
        let reason = wait_until_stalled(&last_event_at, &window, probe, "test").await;
        assert_eq!(reason, StallReason::ProbeFailed);
    }

    /// spawn 需要 'static，把引用参数装入 Arc 后的所有化包装（仅测试用）。
    async fn wait_until_stalled_owned<F, Fut>(
        last_event_at: Arc<Mutex<Option<Instant>>>,
        window: Arc<Mutex<u64>>,
        probe: F,
    ) -> StallReason
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        wait_until_stalled(&last_event_at, &window, probe, "test").await
    }
}
