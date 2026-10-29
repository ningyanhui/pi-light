// SPDX-License-Identifier: Apache-2.0
//
// Session-level Trace / Token insights (M3′) — parsed from Pi RPC only.

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
            } => {
                if let Some(step) = self
                    .trace_steps
                    .iter_mut()
                    .find(|s| s.tool_call_id == *tool_call_id)
                {
                    step.tool_name = tool_name.clone();
                    step.running = true;
                } else {
                    self.trace_steps.push(TraceStep {
                        tool_call_id: tool_call_id.clone(),
                        tool_name: tool_name.clone(),
                        running: true,
                        is_error: false,
                        duration_ms: None,
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
    }
}
