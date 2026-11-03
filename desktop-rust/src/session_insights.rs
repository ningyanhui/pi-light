// SPDX-License-Identifier: Apache-2.0
//
// Session-level Trace / Token insights (M3′) — parsed from Pi RPC only.

use std::time::{Duration, Instant};

use crate::pi_rpc::{PiRpcEvent, SessionStats, TokenUsage, TraceStep};

/// How much token usage we can honestly show in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageDisplayState {
    /// No usage fields observed yet and no successful session stats.
    Unknown,
    /// At least one numeric usage field is available (stream and/or stats).
    Available,
    /// `get_session_stats` failed in a way that suggests the command is unusable here.
    Unsupported,
}

/// How much tool trace we can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceDisplayState {
    /// No tool execution events in this session yet.
    Empty,
    /// At least one tool step recorded.
    Available,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatsFetchState {
    #[default]
    Idle,
    Pending,
    Ready,
    Failed,
}

#[derive(Default, Debug)]
pub struct SessionInsights {
    /// Latest cumulative usage from `message_update.usage` (when Pi sends fields).
    pub live_usage: TokenUsage,
    /// Last successful `get_session_stats` payload (if any).
    pub session_stats: Option<SessionStats>,
    pub stats_fetch: StatsFetchState,
    pub trace_steps: Vec<TraceStep>,
    turn_started_at: Option<Instant>,
    pub last_elapsed: Option<Duration>,
    pub last_tps: Option<f64>,
}

impl SessionInsights {
    pub fn usage_display_state(&self) -> UsageDisplayState {
        if self.stats_fetch == StatsFetchState::Failed && self.effective_usage().is_empty() {
            return UsageDisplayState::Unsupported;
        }
        if self.effective_usage().is_empty() {
            UsageDisplayState::Unknown
        } else {
            UsageDisplayState::Available
        }
    }

    pub fn trace_display_state(&self) -> TraceDisplayState {
        if self.trace_steps.is_empty() {
            TraceDisplayState::Empty
        } else {
            TraceDisplayState::Available
        }
    }

    /// Prefer session stats tokens when present; otherwise live stream usage.
    pub fn effective_usage(&self) -> TokenUsage {
        if let Some(stats) = &self.session_stats {
            if !stats.tokens.is_empty() {
                return stats.tokens.clone();
            }
        }
        self.live_usage.clone()
    }

    pub fn on_stats_request_sent(&mut self) {
        self.stats_fetch = StatsFetchState::Pending;
    }

    pub fn on_turn_start(&mut self) {
        self.turn_started_at = Some(Instant::now());
        self.last_elapsed = None;
        self.last_tps = None;
    }

    pub fn on_turn_end(&mut self) {
        let Some(start) = self.turn_started_at.take() else {
            return;
        };
        let elapsed = start.elapsed();
        self.last_elapsed = Some(elapsed);
        if let Some(out) = self.effective_usage().output {
            let secs = elapsed.as_secs_f64().max(0.001);
            self.last_tps = Some(out as f64 / secs);
        }
    }

    fn compact_count(n: u64) -> String {
        if n >= 10_000 {
            format!("{:.1}K", n as f64 / 1000.0)
        } else {
            n.to_string()
        }
    }

    /// 底栏：Tokens · Trace · tok/s（对齐验收原型）。
    pub fn footer_metrics_line(&self) -> String {
        let tokens = self.tokens_chip();
        let (trace_n, trace_time) = self.trace_chip();
        let tps = self
            .last_tps
            .map(|v| format!("{v:.0} tok/s"))
            .unwrap_or_else(|| "— tok/s".to_owned());
        format!("Tokens {tokens}  ·  Trace {trace_n}调用 · {trace_time}  ·  {tps}")
    }

    fn tokens_chip(&self) -> String {
        let used = self
            .session_stats
            .as_ref()
            .and_then(|s| s.context_tokens)
            .or_else(|| self.effective_usage().total)
            .or_else(
                || match (self.effective_usage().input, self.effective_usage().output) {
                    (Some(a), Some(b)) => Some(a + b),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    _ => None,
                },
            );
        let window = self.session_stats.as_ref().and_then(|s| s.context_window);
        match (used, window) {
            (Some(u), Some(w)) => format!("{}/{}", Self::compact_count(u), Self::compact_count(w)),
            (Some(u), None) => Self::compact_count(u),
            (None, Some(w)) => format!("—/{}", Self::compact_count(w)),
            (None, None) => "—".to_owned(),
        }
    }

    fn trace_chip(&self) -> (usize, String) {
        let n = self.trace_steps.len();
        let ms: u64 = self.trace_steps.iter().filter_map(|s| s.duration_ms).sum();
        let elapsed = self
            .last_elapsed
            .map(|d| d.as_secs_f64())
            .filter(|s| *s > 0.0);
        let time = if ms > 0 {
            format!("{:.1}s", ms as f64 / 1000.0)
        } else if let Some(s) = elapsed {
            format!("{s:.1}s")
        } else {
            "—".to_owned()
        };
        (n, time)
    }

    pub fn handle_rpc_event(&mut self, event: &PiRpcEvent) {
        match event {
            PiRpcEvent::UsageUpdate(usage) => {
                self.live_usage.merge(usage);
            }
            PiRpcEvent::SessionStats(stats) => {
                self.session_stats = Some(stats.clone());
                self.stats_fetch = StatsFetchState::Ready;
            }
            PiRpcEvent::ToolTraceStart {
                tool_call_id,
                tool_name,
                args_preview,
            } => {
                if let Some(step) = self
                    .trace_steps
                    .iter_mut()
                    .find(|s| s.tool_call_id == *tool_call_id)
                {
                    step.tool_name = tool_name.clone();
                    step.running = true;
                    if args_preview.is_some() {
                        step.args_preview = args_preview.clone();
                    }
                } else {
                    self.trace_steps.push(TraceStep {
                        tool_call_id: tool_call_id.clone(),
                        tool_name: tool_name.clone(),
                        running: true,
                        is_error: false,
                        duration_ms: None,
                        args_preview: args_preview.clone(),
                    });
                }
            }
            PiRpcEvent::ToolTraceEnd {
                tool_call_id,
                tool_name,
                is_error,
                duration_ms,
            } => {
                if let Some(step) = self
                    .trace_steps
                    .iter_mut()
                    .find(|s| s.tool_call_id == *tool_call_id)
                {
                    step.tool_name = tool_name.clone();
                    step.running = false;
                    step.is_error = *is_error;
                    step.duration_ms = *duration_ms;
                } else {
                    self.trace_steps.push(TraceStep {
                        tool_call_id: tool_call_id.clone(),
                        tool_name: tool_name.clone(),
                        running: false,
                        is_error: *is_error,
                        duration_ms: *duration_ms,
                        args_preview: None,
                    });
                }
            }
            PiRpcEvent::CommandError { command, .. } if command == "get_session_stats" => {
                self.stats_fetch = StatsFetchState::Failed;
            }
            _ => {}
        }
    }

    /// One-line summary for the usage bar; empty string if nothing to show.
    pub fn usage_summary_line(&self) -> Option<String> {
        let u = self.effective_usage();
        if u.is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        if let Some(v) = u.input {
            parts.push(format!("in {v}"));
        }
        if let Some(v) = u.output {
            parts.push(format!("out {v}"));
        }
        if let Some(v) = u.total {
            parts.push(format!("total {v}"));
        }
        if let Some(v) = u.cache_read {
            parts.push(format!("cache↓ {v}"));
        }
        if let Some(v) = u.cache_write {
            parts.push(format!("cache↑ {v}"));
        }
        if let Some(stats) = &self.session_stats {
            if let Some(cost) = stats.cost {
                parts.push(format!("cost ${cost:.4}"));
            }
            if let Some(p) = stats.context_percent {
                parts.push(format!("ctx {p:.0}%"));
            }
        }
        Some(parts.join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pi_rpc::PiRpcEvent;

    #[test]
    fn usage_unknown_until_fields_arrive() {
        let ins = SessionInsights::default();
        assert_eq!(ins.usage_display_state(), UsageDisplayState::Unknown);
    }

    #[test]
    fn usage_available_from_stream() {
        let mut ins = SessionInsights::default();
        ins.handle_rpc_event(&PiRpcEvent::UsageUpdate(TokenUsage {
            input: None,
            output: Some(12),
            total: Some(24),
            cache_read: None,
            cache_write: None,
        }));
        assert_eq!(ins.usage_display_state(), UsageDisplayState::Available);
        assert!(ins.usage_summary_line().unwrap().contains("out 12"));
    }

    #[test]
    fn stats_failure_without_usage_is_unsupported() {
        let mut ins = SessionInsights::default();
        ins.handle_rpc_event(&PiRpcEvent::CommandError {
            command: "get_session_stats".to_owned(),
            error: "no session".to_owned(),
        });
        assert_eq!(ins.usage_display_state(), UsageDisplayState::Unsupported);
    }

    #[test]
    fn trace_steps_from_tool_events() {
        let mut ins = SessionInsights::default();
        ins.handle_rpc_event(&PiRpcEvent::ToolTraceStart {
            tool_call_id: "c1".to_owned(),
            tool_name: "bash".to_owned(),
            args_preview: Some("{\"command\":\"ls\"}".to_owned()),
        });
        ins.handle_rpc_event(&PiRpcEvent::ToolTraceEnd {
            tool_call_id: "c1".to_owned(),
            tool_name: "bash".to_owned(),
            is_error: false,
            duration_ms: Some(42),
        });
        assert_eq!(ins.trace_display_state(), TraceDisplayState::Available);
        assert_eq!(ins.trace_steps.len(), 1);
        assert!(!ins.trace_steps[0].running);
        assert_eq!(
            ins.trace_steps[0].args_preview.as_deref(),
            Some("{\"command\":\"ls\"}")
        );
    }

    #[test]
    fn footer_metrics_include_tokens_trace_tps() {
        let mut ins = SessionInsights::default();
        ins.on_turn_start();
        ins.handle_rpc_event(&PiRpcEvent::UsageUpdate(TokenUsage {
            input: Some(100),
            output: Some(50),
            total: Some(150),
            cache_read: None,
            cache_write: None,
        }));
        ins.handle_rpc_event(&PiRpcEvent::ToolTraceEnd {
            tool_call_id: "c1".to_owned(),
            tool_name: "bash".to_owned(),
            is_error: false,
            duration_ms: Some(7300),
        });
        ins.on_turn_end();
        let line = ins.footer_metrics_line();
        assert!(line.contains("Tokens"), "{line}");
        assert!(line.contains("Trace 1调用"), "{line}");
        assert!(line.contains("tok/s"), "{line}");
    }
}
