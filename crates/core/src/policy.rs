//! Auto-reply policy: decides whether the AI may answer an incoming message.
//! Pure logic, no I/O. Off by default.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Never reply automatically.
    #[default]
    None,
    /// Only chats the user switched on individually.
    Selected,
    /// Every chat of this type.
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoReplyConfig {
    /// Global kill switch. When false nothing is ever sent automatically.
    pub enabled: bool,
    pub chats: Mode,
    pub groups: Mode,
    /// In groups, only answer messages that mention the user.
    pub groups_only_mentions: bool,
    pub max_per_chat_per_hour: u32,
    /// Human-like delay before answering, in seconds.
    pub min_delay_secs: u32,
    pub max_delay_secs: u32,
    /// Local hours `[from, to)` during which no auto-replies are sent.
    pub quiet_from: Option<u8>,
    pub quiet_to: Option<u8>,
    /// Instructions applied to every auto-reply (tone, what to say).
    pub instructions: String,
}

impl Default for AutoReplyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            chats: Mode::None,
            groups: Mode::None,
            groups_only_mentions: true,
            max_per_chat_per_hour: 6,
            min_delay_secs: 4,
            max_delay_secs: 12,
            quiet_from: None,
            quiet_to: None,
            instructions: String::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Incoming {
    pub is_group: bool,
    pub from_me: bool,
    pub mentions_me: bool,
    /// False for messages drained from the offline queue after reconnecting.
    pub live: bool,
    pub has_text: bool,
    /// Whether this chat is individually switched on.
    pub chat_selected: bool,
    /// Auto-replies already sent to this chat in the last hour.
    pub replies_last_hour: u32,
    /// Local hour of day, 0–23.
    pub hour: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    Disabled,
    OwnMessage,
    NotLive,
    NoText,
    ModeNone,
    NotSelected,
    NotMentioned,
    RateLimited,
    QuietHours,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Reply,
    Skip(Skip),
}

pub fn in_quiet_hours(from: Option<u8>, to: Option<u8>, hour: u8) -> bool {
    match (from, to) {
        (Some(from), Some(to)) if from != to => {
            if from < to {
                hour >= from && hour < to
            } else {
                // Wraps past midnight, e.g. 22 → 7.
                hour >= from || hour < to
            }
        }
        _ => false,
    }
}

pub fn decide(cfg: &AutoReplyConfig, m: &Incoming) -> Decision {
    if !cfg.enabled {
        return Decision::Skip(Skip::Disabled);
    }
    if m.from_me {
        return Decision::Skip(Skip::OwnMessage);
    }
    if !m.live {
        // Never answer a backlog: those messages may be hours old.
        return Decision::Skip(Skip::NotLive);
    }
    if !m.has_text {
        return Decision::Skip(Skip::NoText);
    }
    let mode = if m.is_group { cfg.groups } else { cfg.chats };
    match mode {
        Mode::None => return Decision::Skip(Skip::ModeNone),
        Mode::Selected if !m.chat_selected => return Decision::Skip(Skip::NotSelected),
        _ => {}
    }
    if m.is_group && cfg.groups_only_mentions && !m.mentions_me {
        return Decision::Skip(Skip::NotMentioned);
    }
    if m.replies_last_hour >= cfg.max_per_chat_per_hour {
        return Decision::Skip(Skip::RateLimited);
    }
    if in_quiet_hours(cfg.quiet_from, cfg.quiet_to, m.hour) {
        return Decision::Skip(Skip::QuietHours);
    }
    Decision::Reply
}

/// Picks a delay in `[min, max]` seconds from a random sample in `[0, 1)`.
pub fn delay_secs(cfg: &AutoReplyConfig, sample: f64) -> u32 {
    let (lo, hi) = if cfg.min_delay_secs <= cfg.max_delay_secs {
        (cfg.min_delay_secs, cfg.max_delay_secs)
    } else {
        (cfg.max_delay_secs, cfg.min_delay_secs)
    };
    let span = f64::from(hi - lo);
    lo + (span * sample.clamp(0.0, 1.0)).round() as u32
}

/// Tracks auto-replies per chat inside a sliding one-hour window.
#[derive(Debug, Default)]
pub struct RateWindow {
    sent: std::collections::HashMap<String, Vec<i64>>,
}

impl RateWindow {
    pub fn count(&mut self, chat: &str, now: i64) -> u32 {
        match self.sent.get_mut(chat) {
            Some(times) => {
                times.retain(|t| now - *t < 3600);
                times.len() as u32
            }
            None => 0,
        }
    }

    pub fn record(&mut self, chat: &str, now: i64) {
        self.sent.entry(chat.to_string()).or_default().push(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> AutoReplyConfig {
        AutoReplyConfig {
            enabled: true,
            chats: Mode::All,
            groups: Mode::All,
            ..Default::default()
        }
    }

    fn dm() -> Incoming {
        Incoming {
            is_group: false,
            from_me: false,
            mentions_me: false,
            live: true,
            has_text: true,
            chat_selected: false,
            replies_last_hour: 0,
            hour: 12,
        }
    }

    #[test]
    fn default_config_never_replies() {
        let cfg = AutoReplyConfig::default();
        assert_eq!(decide(&cfg, &dm()), Decision::Skip(Skip::Disabled));
        let enabled_only = AutoReplyConfig {
            enabled: true,
            ..Default::default()
        };
        assert_eq!(decide(&enabled_only, &dm()), Decision::Skip(Skip::ModeNone));
    }

    #[test]
    fn kill_switch_wins_over_everything() {
        let cfg = AutoReplyConfig {
            enabled: false,
            ..on()
        };
        let m = Incoming {
            chat_selected: true,
            ..dm()
        };
        assert_eq!(decide(&cfg, &m), Decision::Skip(Skip::Disabled));
    }

    #[test]
    fn basic_guards() {
        let cfg = on();
        assert_eq!(decide(&cfg, &dm()), Decision::Reply);
        assert_eq!(
            decide(
                &cfg,
                &Incoming {
                    from_me: true,
                    ..dm()
                }
            ),
            Decision::Skip(Skip::OwnMessage)
        );
        assert_eq!(
            decide(
                &cfg,
                &Incoming {
                    live: false,
                    ..dm()
                }
            ),
            Decision::Skip(Skip::NotLive)
        );
        assert_eq!(
            decide(
                &cfg,
                &Incoming {
                    has_text: false,
                    ..dm()
                }
            ),
            Decision::Skip(Skip::NoText)
        );
    }

    #[test]
    fn selected_mode_needs_the_chat_switched_on() {
        let cfg = AutoReplyConfig {
            chats: Mode::Selected,
            ..on()
        };
        assert_eq!(decide(&cfg, &dm()), Decision::Skip(Skip::NotSelected));
        assert_eq!(
            decide(
                &cfg,
                &Incoming {
                    chat_selected: true,
                    ..dm()
                }
            ),
            Decision::Reply
        );
    }

    #[test]
    fn groups_are_independent_and_respect_mentions() {
        let cfg = AutoReplyConfig {
            groups: Mode::None,
            ..on()
        };
        let group = Incoming {
            is_group: true,
            ..dm()
        };
        assert_eq!(decide(&cfg, &group), Decision::Skip(Skip::ModeNone));
        assert_eq!(decide(&cfg, &dm()), Decision::Reply);

        let cfg = on();
        assert_eq!(decide(&cfg, &group), Decision::Skip(Skip::NotMentioned));
        assert_eq!(
            decide(
                &cfg,
                &Incoming {
                    mentions_me: true,
                    ..group
                }
            ),
            Decision::Reply
        );
        let any = AutoReplyConfig {
            groups_only_mentions: false,
            ..on()
        };
        assert_eq!(decide(&any, &group), Decision::Reply);
    }

    #[test]
    fn rate_limit_and_quiet_hours() {
        let cfg = on();
        assert_eq!(
            decide(
                &cfg,
                &Incoming {
                    replies_last_hour: 6,
                    ..dm()
                }
            ),
            Decision::Skip(Skip::RateLimited)
        );
        let quiet = AutoReplyConfig {
            quiet_from: Some(22),
            quiet_to: Some(7),
            ..on()
        };
        assert_eq!(
            decide(&quiet, &Incoming { hour: 23, ..dm() }),
            Decision::Skip(Skip::QuietHours)
        );
        assert_eq!(
            decide(&quiet, &Incoming { hour: 3, ..dm() }),
            Decision::Skip(Skip::QuietHours)
        );
        assert_eq!(
            decide(&quiet, &Incoming { hour: 7, ..dm() }),
            Decision::Reply
        );
        assert!(in_quiet_hours(Some(9), Some(17), 9));
        assert!(!in_quiet_hours(Some(9), Some(17), 17));
        assert!(!in_quiet_hours(Some(9), Some(9), 9));
        assert!(!in_quiet_hours(None, Some(9), 3));
    }

    #[test]
    fn delay_stays_in_range() {
        let cfg = on();
        assert_eq!(delay_secs(&cfg, 0.0), 4);
        assert_eq!(delay_secs(&cfg, 1.0), 12);
        assert_eq!(delay_secs(&cfg, 7.5), 12);
        let swapped = AutoReplyConfig {
            min_delay_secs: 10,
            max_delay_secs: 2,
            ..on()
        };
        assert_eq!(delay_secs(&swapped, 0.0), 2);
    }

    #[test]
    fn rate_window_slides() {
        let mut w = RateWindow::default();
        assert_eq!(w.count("a", 1000), 0);
        w.record("a", 1000);
        w.record("a", 2000);
        assert_eq!(w.count("a", 2500), 2);
        assert_eq!(w.count("b", 2500), 0);
        assert_eq!(w.count("a", 1000 + 3600), 1);
        assert_eq!(w.count("a", 2000 + 3600), 0);
    }
}
