//! Progression rules (D74): XP, levels and badges. Pure, no database.
//!
//! ```text
//! decisions + clones ──count──> ActivityCounts ──xp()──> XP ──level()──> level
//!                                     │
//!              Snapshot (money now) ──┴─earned_milestones()─> "level:4", "badge:first_steps"
//! ```
//!
//! **XP is counted, never stored**, like a player's career statistics: the
//! rows it comes from already exist. Only the moment a milestone was first
//! reached is stored (`avatar_milestones`).
//!
//! **XP is an observer.** Nothing here is ever sent to the LLM, so a
//! character cannot trade "to level up": `decide()` does not know XP exists.
//! Backtest rows never count (the queries read `sim` and `real` only).
//!
//! Level measures *experience* (activity). Skill (returns) stays on the
//! leaderboard; the two numbers are different on purpose.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;

/// XP for every tick that ended with a valid decision (any action).
pub const XP_PER_VALID_TICK: u64 = 5;
/// Extra XP when that decision was a MOVE or a REBALANCE.
pub const XP_PER_MOVE: u64 = 15;
/// Extra XP when that decision was a SAY (an arena message).
pub const XP_PER_MESSAGE: u64 = 10;
/// XP for the source character each time someone clones it.
pub const XP_PER_CLONE: u64 = 100;

/// `rejected_rule` values that are plumbing failures, not guardrails. They
/// do not count for the "Kept in Line" badge. Bound into the stats query.
pub const NON_GUARDRAIL_RULES: [&str; 4] = [
    "parse_error",
    "llm_error",
    "ledger_error",
    "arena_rate_limit",
];

const MILESTONE_LEVEL_PREFIX: &str = "level:";
const MILESTONE_BADGE_PREFIX: &str = "badge:";

const DIVERSIFIED_VILLAGES: usize = 3;
const STORYTELLER_MESSAGES: u64 = 10;
const KEPT_IN_LINE_BLOCKS: u64 = 5;
const VETERAN_DAYS: i64 = 7;
const GUILD_FOUNDER_CLONES: u64 = 5;

/// What a character has done so far, counted from live rows (`sim`, `real`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActivityCounts {
    pub valid_ticks: u64,
    /// Valid MOVE and REBALANCE decisions.
    pub moves: u64,
    /// Valid SAY decisions.
    pub messages: u64,
    /// Proposals refused by a guardrail (see [`NON_GUARDRAIL_RULES`]).
    pub guardrail_blocks: u64,
    /// Characters cloned from this one.
    pub clones: u64,
    /// The first live equity point; `None` before the first tick.
    pub first_live_ts: Option<DateTime<Utc>>,
}

/// The character's money right now, for the badges that look at it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Villages holding more than zero.
    pub villages_held: usize,
    pub equity_usd: Decimal,
    pub starting_capital_usd: Decimal,
    pub now: DateTime<Utc>,
}

/// One badge of the catalog. The API sends `name` and `description`, so the
/// frontend only maps `id` to an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Badge {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
}

/// Every badge, in display order. The unlock rule of each one is in
/// [`earned_badges`]; a test checks that both lists hold the same ids.
pub const BADGES: [Badge; 8] = [
    Badge {
        id: "first_steps",
        name: "First Steps",
        description: "Made a first move between villages.",
    },
    Badge {
        id: "diversified",
        name: "Diversified",
        description: "Held money in 3 villages at the same time.",
    },
    Badge {
        id: "storyteller",
        name: "Storyteller",
        description: "Posted 10 messages in the arena.",
    },
    Badge {
        id: "kept_in_line",
        name: "Kept in Line",
        description: "Stopped by the guardrails 5 times.",
    },
    Badge {
        id: "in_the_green",
        name: "In the Green",
        description: "Grew the starting capital by 1%.",
    },
    Badge {
        id: "veteran",
        name: "Veteran",
        description: "Live in the town for 7 days.",
    },
    Badge {
        id: "masters_mark",
        name: "Master's Mark",
        description: "Cloned by someone for the first time.",
    },
    Badge {
        id: "guild_founder",
        name: "Guild Founder",
        description: "Cloned 5 times.",
    },
];

/// Total XP. Saturating, so absurd counts cannot overflow.
pub fn xp(counts: &ActivityCounts) -> u64 {
    counts
        .valid_ticks
        .saturating_mul(XP_PER_VALID_TICK)
        .saturating_add(counts.moves.saturating_mul(XP_PER_MOVE))
        .saturating_add(counts.messages.saturating_mul(XP_PER_MESSAGE))
        .saturating_add(counts.clones.saturating_mul(XP_PER_CLONE))
}

/// The XP at which `level` starts: `50 · n · (n − 1)`.
/// Level 1 = 0, level 2 = 100, level 3 = 300, level 5 = 1 000, level 10 = 4 500.
pub fn level_start_xp(level: u32) -> u64 {
    let n = u64::from(level);
    50u64.saturating_mul(n).saturating_mul(n.saturating_sub(1))
}

/// The highest level. It starts at about 50 million XP, far beyond what a
/// character can earn; the cap only bounds the loop in [`level`].
pub const MAX_LEVEL: u32 = 999;

/// The level for `xp`. Everyone starts at level 1.
pub fn level(xp: u64) -> u32 {
    let mut level = 1;
    // Each level needs more XP than the last, so this loop is short: about
    // 140 steps for a million XP, and never more than `MAX_LEVEL`.
    while level < MAX_LEVEL && level_start_xp(level + 1) <= xp {
        level += 1;
    }
    level
}

/// Where a character stands inside its level, for the XP bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelProgress {
    pub xp: u64,
    pub level: u32,
    /// XP earned since the current level started.
    pub xp_into_level: u64,
    /// XP between the start of this level and the start of the next one.
    pub xp_for_next_level: u64,
}

pub fn level_progress(counts: &ActivityCounts) -> LevelProgress {
    let xp = xp(counts);
    let level = level(xp);
    let start = level_start_xp(level);
    LevelProgress {
        xp,
        level,
        xp_into_level: xp - start,
        xp_for_next_level: level_start_xp(level.saturating_add(1)).saturating_sub(start),
    }
}

/// The ids of the badges whose rule is true now.
pub fn earned_badges(counts: &ActivityCounts, snapshot: &Snapshot) -> Vec<&'static str> {
    let live_days = counts
        .first_live_ts
        .map_or(0, |first| (snapshot.now - first).num_days());
    // 1% above the starting capital, written without a division.
    let in_the_green = snapshot.equity_usd * Decimal::from(100)
        >= snapshot.starting_capital_usd * Decimal::from(101);

    let mut earned = Vec::new();
    if counts.moves >= 1 {
        earned.push("first_steps");
    }
    if snapshot.villages_held >= DIVERSIFIED_VILLAGES {
        earned.push("diversified");
    }
    if counts.messages >= STORYTELLER_MESSAGES {
        earned.push("storyteller");
    }
    if counts.guardrail_blocks >= KEPT_IN_LINE_BLOCKS {
        earned.push("kept_in_line");
    }
    if in_the_green {
        earned.push("in_the_green");
    }
    if live_days >= VETERAN_DAYS {
        earned.push("veteran");
    }
    if counts.clones >= 1 {
        earned.push("masters_mark");
    }
    if counts.clones >= GUILD_FOUNDER_CLONES {
        earned.push("guild_founder");
    }
    earned
}

/// Every milestone the character should have now, as stored in
/// `avatar_milestones.milestone`: its current level (above 1) and its badges.
///
/// Only the *current* level is listed: a character that jumps from level 1
/// to 4 gets one "reached level 4" milestone, not three.
pub fn earned_milestones(counts: &ActivityCounts, snapshot: &Snapshot) -> Vec<String> {
    let mut milestones = Vec::new();
    let level = level(xp(counts));
    if level > 1 {
        milestones.push(format!("{MILESTONE_LEVEL_PREFIX}{level}"));
    }
    for badge in earned_badges(counts, snapshot) {
        milestones.push(format!("{MILESTONE_BADGE_PREFIX}{badge}"));
    }
    milestones
}

/// A stored milestone, decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Milestone {
    Level(u32),
    Badge(Badge),
}

impl Milestone {
    /// Decodes `"level:4"` or `"badge:first_steps"`. `None` for anything
    /// else, e.g. a badge that was removed from the catalog.
    pub fn parse(stored: &str) -> Option<Self> {
        if let Some(level) = stored.strip_prefix(MILESTONE_LEVEL_PREFIX) {
            return level.parse().ok().map(Self::Level);
        }
        let id = stored.strip_prefix(MILESTONE_BADGE_PREFIX)?;
        BADGES
            .iter()
            .find(|badge| badge.id == id)
            .copied()
            .map(Self::Badge)
    }

    /// `"level"` or `"badge"`, as sent to the browser.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Level(_) => "level",
            Self::Badge(_) => "badge",
        }
    }

    /// Text for the ticker and the Hall of Fame: "Level 4" or "First Steps".
    pub fn label(&self) -> String {
        match self {
            Self::Level(level) => format!("Level {level}"),
            Self::Badge(badge) => badge.name.to_string(),
        }
    }
}

/// The badge id inside a stored milestone, or `None` for a level.
pub fn badge_id(stored: &str) -> Option<&str> {
    stored.strip_prefix(MILESTONE_BADGE_PREFIX)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 10, 12, 0, 0).unwrap()
    }

    /// A character that has done nothing and holds its starting cash.
    fn idle_snapshot() -> Snapshot {
        Snapshot {
            villages_held: 0,
            equity_usd: Decimal::from(1000),
            starting_capital_usd: Decimal::from(1000),
            now: now(),
        }
    }

    #[test]
    fn xp_adds_each_source_with_its_weight() {
        let counts = ActivityCounts {
            valid_ticks: 10,
            moves: 2,
            messages: 3,
            guardrail_blocks: 7,
            clones: 1,
            first_live_ts: None,
        };

        // 10×5 + 2×15 + 3×10 + 1×100; blocks give nothing and take nothing.
        assert_eq!(xp(&counts), 210);
        assert_eq!(xp(&ActivityCounts::default()), 0);
    }

    #[test]
    fn xp_saturates_instead_of_overflowing() {
        let counts = ActivityCounts {
            valid_ticks: u64::MAX,
            clones: u64::MAX,
            ..ActivityCounts::default()
        };

        assert_eq!(xp(&counts), u64::MAX);
        assert_eq!(level(u64::MAX), MAX_LEVEL, "no panic, no endless loop");
    }

    #[test]
    fn levels_change_exactly_at_their_threshold() {
        assert_eq!(level_start_xp(1), 0);
        assert_eq!(level_start_xp(2), 100);
        assert_eq!(level_start_xp(3), 300);
        assert_eq!(level_start_xp(5), 1_000);
        assert_eq!(level_start_xp(10), 4_500);

        for (xp, expected) in [
            (0, 1),
            (99, 1),
            (100, 2),
            (299, 2),
            (300, 3),
            (999, 4),
            (1_000, 5),
            (4_499, 9),
            (4_500, 10),
        ] {
            assert_eq!(level(xp), expected, "xp = {xp}");
        }
    }

    #[test]
    fn level_progress_measures_the_current_level_only() {
        // 150 XP: level 2 runs from 100 to 300.
        let counts = ActivityCounts {
            valid_ticks: 30,
            ..ActivityCounts::default()
        };

        assert_eq!(
            level_progress(&counts),
            LevelProgress {
                xp: 150,
                level: 2,
                xp_into_level: 50,
                xp_for_next_level: 200,
            }
        );
    }

    #[test]
    fn an_idle_character_has_no_milestone() {
        let counts = ActivityCounts::default();

        assert!(earned_badges(&counts, &idle_snapshot()).is_empty());
        assert!(earned_milestones(&counts, &idle_snapshot()).is_empty());
    }

    #[test]
    fn each_badge_unlocks_at_its_limit_and_not_just_below() {
        let seven_days_ago = now() - chrono::Duration::days(7);
        let almost_seven_days_ago = seven_days_ago + chrono::Duration::seconds(1);
        let with = |change: fn(&mut ActivityCounts)| {
            let mut counts = ActivityCounts::default();
            change(&mut counts);
            counts
        };
        let holding = |villages_held: usize, equity: &str| Snapshot {
            villages_held,
            equity_usd: Decimal::from_str_exact(equity).unwrap(),
            ..idle_snapshot()
        };

        // (badge, counts, snapshot, expected)
        let cases: Vec<(&str, ActivityCounts, Snapshot, bool)> = vec![
            ("first_steps", with(|c| c.moves = 1), idle_snapshot(), true),
            ("diversified", ActivityCounts::default(), holding(2, "1000"), false),
            ("diversified", ActivityCounts::default(), holding(3, "1000"), true),
            ("storyteller", with(|c| c.messages = 9), idle_snapshot(), false),
            ("storyteller", with(|c| c.messages = 10), idle_snapshot(), true),
            ("kept_in_line", with(|c| c.guardrail_blocks = 4), idle_snapshot(), false),
            ("kept_in_line", with(|c| c.guardrail_blocks = 5), idle_snapshot(), true),
            ("in_the_green", ActivityCounts::default(), holding(0, "1009.99"), false),
            ("in_the_green", ActivityCounts::default(), holding(0, "1010"), true),
            ("masters_mark", with(|c| c.clones = 1), idle_snapshot(), true),
            ("guild_founder", with(|c| c.clones = 4), idle_snapshot(), false),
            ("guild_founder", with(|c| c.clones = 5), idle_snapshot(), true),
        ];
        for (badge, counts, snapshot, expected) in cases {
            assert_eq!(
                earned_badges(&counts, &snapshot).contains(&badge),
                expected,
                "{badge} with {counts:?} and {snapshot:?}"
            );
        }

        let veteran = |first: DateTime<Utc>| {
            let counts = ActivityCounts {
                first_live_ts: Some(first),
                ..ActivityCounts::default()
            };
            earned_badges(&counts, &idle_snapshot()).contains(&"veteran")
        };
        assert!(veteran(seven_days_ago));
        assert!(!veteran(almost_seven_days_ago));
    }

    /// The catalog and the rules must name the same badges: a badge with no
    /// rule could never unlock, and a rule with no catalog entry has no name.
    #[test]
    fn every_catalog_badge_has_a_rule_and_every_rule_a_catalog_entry() {
        let everything = ActivityCounts {
            valid_ticks: 1_000,
            moves: 100,
            messages: 100,
            guardrail_blocks: 100,
            clones: 100,
            first_live_ts: Some(now() - chrono::Duration::days(30)),
        };
        let rich = Snapshot {
            villages_held: 4,
            equity_usd: Decimal::from(2000),
            ..idle_snapshot()
        };

        let earned = earned_badges(&everything, &rich);

        let catalog: Vec<&str> = BADGES.iter().map(|badge| badge.id).collect();
        assert_eq!(earned, catalog);
    }

    #[test]
    fn milestones_hold_the_current_level_and_the_badges() {
        // 20 ticks + 1 move = 115 XP = level 2.
        let counts = ActivityCounts {
            valid_ticks: 20,
            moves: 1,
            ..ActivityCounts::default()
        };

        assert_eq!(
            earned_milestones(&counts, &idle_snapshot()),
            vec!["level:2".to_string(), "badge:first_steps".to_string()]
        );
    }

    #[test]
    fn stored_milestones_are_decoded_and_unknown_ones_refused() {
        assert_eq!(Milestone::parse("level:4"), Some(Milestone::Level(4)));
        assert_eq!(Milestone::parse("level:4").unwrap().label(), "Level 4");
        assert_eq!(Milestone::parse("level:4").unwrap().kind(), "level");

        let badge = Milestone::parse("badge:first_steps").unwrap();
        assert_eq!(badge.label(), "First Steps");
        assert_eq!(badge.kind(), "badge");

        for unknown in ["badge:removed_badge", "level:x", "quest:1", ""] {
            assert_eq!(Milestone::parse(unknown), None, "{unknown}");
        }
        assert_eq!(badge_id("badge:veteran"), Some("veteran"));
        assert_eq!(badge_id("level:2"), None);
    }

    /// Stored milestones must pass the table's CHECK: lowercase ids only.
    #[test]
    fn badge_ids_fit_the_database_check() {
        for badge in BADGES {
            assert!(
                badge.id.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}",
                badge.id
            );
        }
    }
}
